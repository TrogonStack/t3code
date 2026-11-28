//! Which modifier has both sides held. GPUI's modifier events don't say which side changed (the
//! web reads `event.code`), so this reads the device-level flags macOS keeps per side.

use crate::settings_snapshot::Modifier;

pub fn held_pair() -> Option<Modifier> {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGEventSourceFlagsState(state_id: i32) -> u64;
        }
        const HID_SYSTEM_STATE: i32 = 1;
        // SAFETY: takes a state id and only reads the current modifier flags.
        pair_from_flags(unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) })
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// `NX_DEVICE*KEYMASK` bits from IOKit's `IOLLEvent.h`, as (left, right).
fn pair_from_flags(flags: u64) -> Option<Modifier> {
    [
        (Modifier::Shift, 0x2, 0x4),
        (Modifier::Control, 0x1, 0x2000),
        (Modifier::Alt, 0x20, 0x40),
        (Modifier::Meta, 0x8, 0x10),
    ]
    .into_iter()
    .find(|&(_, left, right)| flags & left != 0 && flags & right != 0)
    .map(|(modifier, ..)| modifier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_needs_both_sides_of_one_modifier() {
        for (flags, expected) in [
            (0x2 | 0x4, Some(Modifier::Shift)),
            (0x1 | 0x2000, Some(Modifier::Control)),
            (0x20 | 0x40, Some(Modifier::Alt)),
            (0x8 | 0x10, Some(Modifier::Meta)),
            (0x2, None),
            (0x2 | 0x10, None),
            (0x2_0000 | 0x2, None),
        ] {
            assert_eq!(pair_from_flags(flags), expected, "{flags:#x}");
        }
    }
}
