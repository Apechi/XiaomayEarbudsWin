// Blocking helpers for WinRT async operations (poll Status with a timeout,
// no async runtime needed inside the session thread).

use std::time::{Duration, Instant};

use windows::core::{Error, HRESULT, Result, RuntimeType};
use windows_future::{IAsyncAction, IAsyncOperation};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn wait_op<T: RuntimeType + 'static>(
    op: IAsyncOperation<T>,
    timeout: Duration,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    loop {
        match op.Status()? {
            s if s == AsyncStatus::Completed => return op.GetResults(),
            s if s == AsyncStatus::Started => {
                if Instant::now() >= deadline {
                    let _ = op.Cancel();
                    return Err(Error::new(
                        HRESULT(-1),
                        "WinRT async operation timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            s if s == AsyncStatus::Error => {
                let hr = op.ErrorCode()?;
                return Err(Error::from_hresult(hr));
            }
            _ => {
                return Err(Error::new(HRESULT(-1), "WinRT async operation canceled"));
            }
        }
    }
}

pub fn wait_action(op: IAsyncAction, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match op.Status()? {
            s if s == AsyncStatus::Completed => return Ok(()),
            s if s == AsyncStatus::Started => {
                if Instant::now() >= deadline {
                    let _ = op.Cancel();
                    return Err(Error::new(
                        HRESULT(-1),
                        "WinRT async operation timed out",
                    ));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            s if s == AsyncStatus::Error => {
                let hr = op.ErrorCode()?;
                return Err(Error::from_hresult(hr));
            }
            _ => {
                return Err(Error::new(HRESULT(-1), "WinRT async operation canceled"));
            }
        }
    }
}

/// Block until the operation completes, with no timeout. Use on dedicated
/// reader threads where idling indefinitely is normal (never cancels the op —
/// canceling a DataReader operation poisons the reader).
pub fn wait_op_forever<T: RuntimeType + 'static>(op: IAsyncOperation<T>) -> Result<T> {
    loop {
        match op.Status()? {
            s if s == AsyncStatus::Completed => return op.GetResults(),
            s if s == AsyncStatus::Started => {
                std::thread::sleep(Duration::from_millis(10));
            }
            s if s == AsyncStatus::Error => {
                let hr = op.ErrorCode()?;
                return Err(Error::from_hresult(hr));
            }
            _ => {
                return Err(Error::new(HRESULT(-1), "WinRT async operation canceled"));
            }
        }
    }
}

use windows_future::AsyncStatus;
