//! A per-process one-shot timer. No global timer resolution changes or busy wait.
use std::time::Duration;
use windows::{
    core::*,
    Win32::{Foundation::*, System::Threading::*},
};

pub struct FrameTimer {
    pub handle: HANDLE,
    pub high_resolution: bool,
}
impl FrameTimer {
    pub unsafe fn new() -> Result<Self> {
        let high = CreateWaitableTimerExW(
            None,
            None,
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS.0,
        );
        let high_resolution = high.is_ok();
        let handle = high.or_else(|_| CreateWaitableTimerExW(None, None, 0, TIMER_ALL_ACCESS.0))?;
        Ok(Self {
            handle,
            high_resolution,
        })
    }
    pub unsafe fn arm(&self, delay: Duration) -> Result<()> {
        let ticks = (delay.as_nanos() / 100).clamp(1, i64::MAX as u128) as i64;
        SetWaitableTimer(self.handle, &-ticks, 0, None, None, false)
    }
    pub unsafe fn cancel(&self) {
        let _ = CancelWaitableTimer(self.handle);
    }
}
impl Drop for FrameTimer {
    fn drop(&mut self) {
        unsafe {
            self.cancel();
            let _ = CloseHandle(self.handle);
        }
    }
}
