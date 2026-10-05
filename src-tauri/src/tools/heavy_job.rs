use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use tracing::warn;

const HEAVY_LOG_TARGET: &str = "veyro.tools.heavy";

/// Stack size for in-process ML/decode stages (matches sherpa worker threads).
pub const STAGE_STACK: usize = 16 * 1024 * 1024;

static STAGE_ACTIVE: AtomicBool = AtomicBool::new(false);

fn begin_stage(name: &'static str) -> Result<(), String> {
    if STAGE_ACTIVE.swap(true, Ordering::SeqCst) {
        STAGE_ACTIVE.store(false, Ordering::SeqCst);
        warn!(
            target: HEAVY_LOG_TARGET,
            stage = name,
            "nested run_stage call rejected"
        );
        return Err(format!("tools.failed|nested heavy stage: {name}"));
    }
    Ok(())
}

fn end_stage() {
    STAGE_ACTIVE.store(false, Ordering::SeqCst);
}

/// Run a synchronous heavy stage on a dedicated std thread with a large stack.
pub async fn run_stage<T>(
    name: &'static str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    run_stage_with_stack(name, STAGE_STACK, work).await
}

/// Like [`run_stage`] with an explicit thread stack size.
pub async fn run_stage_with_stack<T>(
    name: &'static str,
    stack_size: usize,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    begin_stage(name)?;

    let (tx, rx) = tokio::sync::oneshot::channel();
    thread::Builder::new()
        .name(format!("veyro-heavy-{name}"))
        .stack_size(stack_size)
        .spawn(move || {
            log_stage_stack_limits(name, stack_size);
            let result = work();
            end_stage();
            let _ = tx.send(result);
        })
        .map_err(|error| {
            end_stage();
            format!("tools.failed|failed to spawn heavy stage {name}: {error}")
        })?;

    rx.await
        .map_err(|_| format!("tools.failed|heavy stage {name} dropped"))?
}

#[cfg(windows)]
fn log_stage_stack_limits(stage: &'static str, requested_stack: usize) {
    use std::mem::MaybeUninit;

    use windows::core::PCSTR;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    type GetCurrentThreadStackLimits =
        unsafe extern "system" fn(*mut usize, *mut usize) -> i32;

    unsafe {
        let module = GetModuleHandleA(PCSTR(b"kernel32.dll\0".as_ptr())).ok();
        let Some(module) = module else {
            return;
        };
        let proc = GetProcAddress(module, PCSTR(b"GetCurrentThreadStackLimits\0".as_ptr()));
        let Some(proc) = proc else {
            return;
        };
        let get_limits: GetCurrentThreadStackLimits = std::mem::transmute(proc);
        let mut low = MaybeUninit::<usize>::uninit();
        let mut high = MaybeUninit::<usize>::uninit();
        if get_limits(low.as_mut_ptr(), high.as_mut_ptr()) != 0 {
            let low = low.assume_init();
            let high = high.assume_init();
            let reserve = high.saturating_sub(low);
            if reserve < requested_stack.saturating_sub(1024 * 1024) {
                warn!(
                    target: HEAVY_LOG_TARGET,
                    stage,
                    reserve_bytes = reserve,
                    requested = requested_stack,
                    "thread stack reserve below requested stack_size"
                );
            }
        }
    }
}

#[cfg(not(windows))]
fn log_stage_stack_limits(_stage: &'static str, _requested_stack: usize) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn run_stage_returns_value() {
        let value = run_stage("test", || Ok(42usize)).await.expect("stage");
        assert_eq!(value, 42);
    }

    #[cfg(windows)]
    #[test]
    fn stage_thread_stack_at_least_15_mib() {
        use std::sync::mpsc;

        use windows::core::PCSTR;
        use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .stack_size(STAGE_STACK)
            .spawn(move || {
                let mut low = 0usize;
                let mut high = 0usize;
                unsafe {
                    type FnLimits = unsafe extern "system" fn(*mut usize, *mut usize) -> i32;
                    let module = GetModuleHandleA(PCSTR(b"kernel32.dll\0".as_ptr())).ok();
                    let Some(module) = module else {
                        let _ = tx.send(None);
                        return;
                    };
                    let proc =
                        GetProcAddress(module, PCSTR(b"GetCurrentThreadStackLimits\0".as_ptr()));
                    let Some(proc) = proc else {
                        let _ = tx.send(None);
                        return;
                    };
                    let get_limits: FnLimits = std::mem::transmute(proc);
                    if get_limits(&mut low, &mut high) != 0 {
                        let _ = tx.send(None);
                        return;
                    }
                }
                let reserve = high.saturating_sub(low);
                let _ = tx.send(Some(reserve));
            })
            .expect("spawn");

        let reserve = rx.recv_timeout(std::time::Duration::from_secs(5)).ok().flatten();
        if let Some(reserve) = reserve {
            assert!(
                reserve >= 15 * 1024 * 1024,
                "expected >= 15 MiB stack reserve, got {reserve}"
            );
        }
    }
}
