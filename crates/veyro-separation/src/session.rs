#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionProvider {
    #[default]
    Cpu,
    Cuda,
    DirectMl,
    CoreMl,
}

impl ExecutionProvider {
    pub fn from_str(raw: &str) -> Self {
        match raw.to_ascii_lowercase().as_str() {
            "cuda" => Self::Cuda,
            "directml" => Self::DirectMl,
            "coreml" => Self::CoreMl,
            _ => Self::Cpu,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::DirectMl => "directml",
            Self::CoreMl => "coreml",
        }
    }

    pub fn execution_providers(self) -> Vec<ort::ep::ExecutionProviderDispatch> {
        match self {
            Self::Cpu => vec![ort::ep::CPU::default().build()],
            #[cfg(feature = "cuda")]
            Self::Cuda => vec![
                ort::ep::CUDA::default().build(),
                ort::ep::CPU::default().build(),
            ],
            #[cfg(not(feature = "cuda"))]
            Self::Cuda => vec![ort::ep::CPU::default().build()],
            #[cfg(feature = "directml")]
            Self::DirectMl => vec![
                ort::ep::DirectML::default().build(),
                ort::ep::CPU::default().build(),
            ],
            #[cfg(not(feature = "directml"))]
            Self::DirectMl => vec![ort::ep::CPU::default().build()],
            #[cfg(feature = "coreml")]
            Self::CoreMl => vec![
                ort::ep::CoreML::default().build(),
                ort::ep::CPU::default().build(),
            ],
            #[cfg(not(feature = "coreml"))]
            Self::CoreMl => vec![ort::ep::CPU::default().build()],
        }
    }
}

pub struct SessionOptions {
    pub provider: ExecutionProvider,
    pub intra_threads: usize,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            provider: ExecutionProvider::Cpu,
            intra_threads: 4,
        }
    }
}
