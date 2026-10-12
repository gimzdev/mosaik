//! A browser's tabs on screen, found with UI Automation, so a probe can drag one out the way a
//! person does.

use mosaik_core::geometry::Rect;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_TabItemControlTypeId,
};

/// The tabs of the browser window `hwnd`, left to right (empty when the browser exposes none).
/// Needs COM on this thread.
pub fn tab_rects(hwnd: HWND) -> windows::core::Result<Vec<Rect>> {
    // SAFETY: COM calls on this thread with a window handle the probe opened; results are owned
    // interface pointers and plain structs.
    unsafe {
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let root = uia.ElementFromHandle(hwnd)?;
        let all = root.FindAll(TreeScope_Descendants, &uia.CreateTrueCondition()?)?;
        let mut tabs = Vec::new();
        for i in 0..all.Length()? {
            let e = all.GetElement(i)?;
            if e.CurrentControlType()? != UIA_TabItemControlTypeId {
                continue;
            }
            let r = e.CurrentBoundingRectangle()?;
            if r.right > r.left && r.bottom > r.top {
                tabs.push(Rect::new(r.left, r.top, r.right, r.bottom));
            }
        }
        tabs.sort_by_key(|r| (r.top, r.left));
        Ok(tabs)
    }
}
