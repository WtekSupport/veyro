use std::sync::{Mutex, OnceLock};

#[cfg(feature = "silero-te")]
use std::collections::HashSet;

#[cfg(feature = "silero-te")]
use serde::Deserialize;

#[cfg(feature = "silero-te")]
use super::process::{
    build_uni_symbols, enhance_tokens, language_code, language_index, process_unicode,
    unitoken_into_token,
};
#[cfg(feature = "silero-te")]
use super::store;

static ENGINE: OnceLock<Mutex<SileroTeEngine>> = OnceLock::new();

pub struct SileroTeEngine {
    #[cfg(feature = "silero-te")]
    inner: Mutex<Option<LoadedEngine>>,
    settings_fingerprint: Mutex<Option<String>>,
}

#[cfg(feature = "silero-te")]
struct LoadedEngine {
    model: tch::CModule,
    tokenizer: tch::CModule,
    uni_symbols: HashSet<char>,
    pad: bool,
    pad_token: String,
}

#[cfg(feature = "silero-te")]
#[derive(Deserialize)]
struct TeMeta {
    pad: bool,
    pad_token: String,
    uni_vocab: Vec<String>,
}

impl SileroTeEngine {
    pub fn global() -> &'static Mutex<SileroTeEngine> {
        ENGINE.get_or_init(|| Mutex::new(SileroTeEngine::new()))
    }

    fn new() -> Self {
        Self {
            #[cfg(feature = "silero-te")]
            inner: Mutex::new(None),
            settings_fingerprint: Mutex::new(None),
        }
    }

    pub fn is_loaded(&self) -> bool {
        #[cfg(feature = "silero-te")]
        {
            self.inner
                .lock()
                .ok()
                .is_some_and(|guard| guard.is_some())
        }
        #[cfg(not(feature = "silero-te"))]
        {
            false
        }
    }

    pub fn unload(&self) {
        #[cfg(feature = "silero-te")]
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        if let Ok(mut fp) = self.settings_fingerprint.lock() {
            *fp = None;
        }
    }

    pub fn enhance(&self, text: &str, language: &str) -> Result<String, String> {
        #[cfg(not(feature = "silero-te"))]
        {
            let _ = (text, language);
            return Err("Silero TE is not compiled into this build".to_string());
        }

        #[cfg(feature = "silero-te")]
        {
            let settings = crate::settings::load_settings().map_err(|error| error.to_string())?;
            self.ensure_loaded(&settings)?;
            let lang = language_code(language);
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "Silero TE engine lock poisoned".to_string())?;
            let loaded = guard
                .as_mut()
                .ok_or_else(|| "Silero TE model is not loaded".to_string())?;
            enhance_with_loaded(loaded, text, lang)
        }
    }

    #[cfg(feature = "silero-te")]
    fn ensure_loaded(&self, settings: &crate::settings::AppSettings) -> Result<(), String> {
        let fingerprint = crate::settings::resolve_dictionary_file_path(settings)
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let needs_reload = self
            .settings_fingerprint
            .lock()
            .ok()
            .map(|fp| fp.as_deref() != Some(fingerprint.as_str()))
            .unwrap_or(true);

        if !needs_reload && self.is_loaded() {
            return Ok(());
        }

        let assets = store::resolve_assets(settings)?;
        let meta: TeMeta = serde_json::from_str(
            &std::fs::read_to_string(&assets.meta).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let uni_symbols = build_uni_symbols(&meta.uni_vocab);

        let model = tch::CModule::load(&assets.model).map_err(|error| error.to_string())?;
        let tokenizer = tch::CModule::load(&assets.tokenizer).map_err(|error| error.to_string())?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "Silero TE engine lock poisoned".to_string())?;
        *guard = Some(LoadedEngine {
            model,
            tokenizer,
            uni_symbols,
            pad: meta.pad,
            pad_token: meta.pad_token,
        });
        if let Ok(mut fp) = self.settings_fingerprint.lock() {
            *fp = Some(fingerprint);
        }
        Ok(())
    }
}

#[cfg(feature = "silero-te")]
fn tensor_from_ivalue(value: tch::IValue) -> Result<tch::Tensor, String> {
    match value {
        tch::IValue::Tensor(tensor) => Ok(tensor),
        other => Err(format!("expected tensor output, got {other:?}")),
    }
}

#[cfg(feature = "silero-te")]
fn string_from_ivalue(value: tch::IValue) -> Result<String, String> {
    match value {
        tch::IValue::String(text) => Ok(text),
        other => Err(format!("expected string output, got {other:?}")),
    }
}

#[cfg(feature = "silero-te")]
fn enhance_with_loaded(loaded: &mut LoadedEngine, text: &str, lang: &str) -> Result<String, String> {
    const LEN_LIMIT: usize = 150;
    let words: Vec<&str> = text.split_whitespace().collect();
    let enhanced = if words.len() < LEN_LIMIT {
        enhance_block(loaded, text, lang)?
    } else {
        enhance_long(loaded, text, lang, LEN_LIMIT)?
    };

    Ok(enhanced.replace('_', " -").trim().to_string())
}

#[cfg(feature = "silero-te")]
fn enhance_long(
    loaded: &mut LoadedEngine,
    text: &str,
    lang: &str,
    len_limit: usize,
) -> Result<String, String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = String::new();
    let mut from = 0usize;
    while from < words.len() {
        let to = (from + len_limit).min(words.len());
        let block = words[from..to].join(" ");
        let enhanced = enhance_block(loaded, &block, lang)?;
        let enhanced_words: Vec<&str> = enhanced.split_whitespace().collect();
        let symbols: String = enhanced_words
            .iter()
            .filter_map(|word| word.chars().last())
            .collect();
        let mut ind = symbols
            .rfind(['.', '!', '?'])
            .map(|index| index + 1)
            .unwrap_or(enhanced_words.len());
        ind += enhanced.matches('-').count();
        ind = ind.min(enhanced_words.len());
        if ind > 0 {
            result.push_str(&enhanced_words[..ind].join(" "));
            result.push(' ');
        }
        from += ind.max(1);
    }
    Ok(result)
}

#[cfg(feature = "silero-te")]
fn enhance_block(loaded: &mut LoadedEngine, text: &str, lang: &str) -> Result<String, String> {
    use tch::{IValue, Kind, Tensor};

    let processed = process_unicode(text, &loaded.uni_symbols);
    let ids_value = loaded
        .tokenizer
        .method_is("convert_string_to_ids", &[IValue::String(processed)])
        .map_err(|error| error.to_string())?;
    let ids = tensor_from_ivalue(ids_value)?;

    let (ids, pad, att_mask) = pad_ids(loaded, &ids)?;
    let lan_id = Tensor::from_slice(&[language_index(lang)])
        .reshape([1, 1, 1])
        .to_kind(Kind::Int64);

    let outputs = loaded
        .model
        .method_is(
            "forward",
            &[
                IValue::Tensor(ids.shallow_clone()),
                IValue::Tensor(att_mask),
                IValue::Tensor(lan_id),
            ],
        )
        .map_err(|error| error.to_string())?;

    let (punct, capital) = match outputs {
        IValue::Tuple(mut values) if values.len() == 2 => {
            let cap = tensor_from_ivalue(values.pop().unwrap())?;
            let punct = tensor_from_ivalue(values.pop().unwrap())?;
            (punct, cap)
        }
        _ => return Err("unexpected Silero TE model output".to_string()),
    };

    let punct_idx = punct.argmax(-1, false);
    let capital_idx = capital.argmax(-1, false);

    let ids_vec: Vec<i64> = ids
        .reshape(-1)
        .try_into()
        .map_err(|_| "failed to flatten token ids".to_string())?;
    let tokens_value = loaded
        .tokenizer
        .method_is("convert_ids_to_tokens", &[IValue::IntList(ids_vec)])
        .map_err(|error| error.to_string())?;

    let tokens: Vec<String> = match tokens_value {
        IValue::GenericList(list) => list
            .into_iter()
            .filter_map(|value| match value {
                IValue::String(token) => Some(unitoken_into_token(&token)),
                _ => None,
            })
            .collect(),
        IValue::StringList(list) => list
            .into_iter()
            .map(|token| unitoken_into_token(&token))
            .collect(),
        other => {
            return Err(format!("unexpected tokenizer token list: {other:?}"))
        }
    };

    let punct_vec: Vec<i64> = punct_idx
        .reshape(-1)
        .try_into()
        .map_err(|_| "failed to flatten punctuation indices".to_string())?;
    let capital_vec: Vec<i64> = capital_idx
        .reshape(-1)
        .try_into()
        .map_err(|_| "failed to flatten capitalization indices".to_string())?;

    let (tokens, punct_vec, capital_vec) = if pad {
        trim_padded_tokens(loaded, tokens, punct_vec, capital_vec)?
    } else {
        (tokens, punct_vec, capital_vec)
    };

    let inner_tokens = if tokens.len() > 2 {
        &tokens[1..tokens.len() - 1]
    } else {
        tokens.as_slice()
    };
    let inner_punct = if punct_vec.len() > 2 {
        &punct_vec[1..punct_vec.len() - 1]
    } else {
        punct_vec.as_slice()
    };
    let inner_capital = if capital_vec.len() > 2 {
        &capital_vec[1..capital_vec.len() - 1]
    } else {
        capital_vec.as_slice()
    };

    let index2punct = [
        (1, '.'),
        (2, ','),
        (3, '-'),
        (4, '!'),
        (5, '?'),
        (6, '_'),
    ];
    let pieces = enhance_tokens(inner_tokens, inner_punct, inner_capital, &index2punct);
    let joined = loaded
        .tokenizer
        .method_is(
            "convert_tokens_to_string",
            &[IValue::GenericList(
                pieces.into_iter().map(IValue::String).collect(),
            )],
        )
        .map_err(|error| error.to_string())?;
    string_from_ivalue(joined)
}

#[cfg(feature = "silero-te")]
fn pad_ids(
    loaded: &LoadedEngine,
    ids: &tch::Tensor,
) -> Result<(tch::Tensor, bool, tch::Tensor), String> {
    use tch::{Kind, Tensor};

    if !loaded.pad {
        let att = Tensor::ones_like(ids);
        return Ok((ids.shallow_clone(), false, att));
    }

    const LIMIT: i64 = 18;
    let seq_len = ids.size()[1];
    let out_len = if seq_len < LIMIT {
        LIMIT
    } else {
        (seq_len + LIMIT).min(512)
    };

    let padded = Tensor::zeros([1, out_len], (Kind::Int64, ids.device()));
    let copy_len = (seq_len - 1).max(0);
    if copy_len > 0 {
        padded
            .slice(1, 0, copy_len, 1)
            .copy_(&ids.slice(1, 0, copy_len, 1));
    }
    if seq_len > 0 {
        let last = ids.select(1, seq_len - 1);
        padded.slice(1, out_len - 1, out_len, 1).copy_(&last);
    }

    let att_mask = Tensor::ones_like(&padded);
    if seq_len > 0 {
        let _ = att_mask
            .slice(1, seq_len - 1, out_len - 1, 1)
            .fill_(0);
    }

    Ok((padded, true, att_mask))
}

#[cfg(feature = "silero-te")]
fn trim_padded_tokens(
    loaded: &LoadedEngine,
    tokens: Vec<String>,
    punct: Vec<i64>,
    capital: Vec<i64>,
) -> Result<(Vec<String>, Vec<i64>, Vec<i64>), String> {
    let pad = &loaded.pad_token;
    if let Some(index) = tokens.iter().position(|token| token == pad) {
        let mut trimmed = tokens[..index].to_vec();
        if let Some(last) = tokens.last() {
            trimmed.push(last.clone());
        }
        let keep = trimmed.len();
        Ok((
            trimmed,
            punct.into_iter().take(keep).collect(),
            capital.into_iter().take(keep).collect(),
        ))
    } else {
        Ok((tokens, punct, capital))
    }
}
