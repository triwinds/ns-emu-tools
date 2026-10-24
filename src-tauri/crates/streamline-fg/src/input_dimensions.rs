//! Input sizing shared by DLSS and independent XeSS.
use crate::advanced_settings::InputSizing;
use ash::vk;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn resized(size: vk::Extent2D, sizing: InputSizing) -> Result<vk::Extent2D> {
    if !sizing.valid() {
        return Err("invalid shared input sizing".into());
    }
    let (percent, cap) = (sizing.scale_percent, sizing.max_edge);
    let longest = size.width.max(size.height);
    if size.width == 0 || size.height == 0 || longest > 8192 {
        return Err("input scaling extent exceeds limits".into());
    }
    let (n, d) = if cap == 0 {
        (percent, 100)
    } else {
        (cap.min(longest), longest)
    };
    let low = vk::Extent2D {
        width: (u64::from(size.width) * u64::from(n) / u64::from(d)) as u32,
        height: (u64::from(size.height) * u64::from(n) / u64::from(d)) as u32,
    };
    if low != size && (low.width < 320 || low.height < 180) {
        return Err("scaled input must be at least 320x180".into());
    }
    Ok(low)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_preserve_aspect_and_never_enlarge() {
        let size = vk::Extent2D {
            width: 1920,
            height: 1080,
        };
        assert_eq!(resized(size, InputSizing::default()).unwrap(), size);
        assert_eq!(
            resized(
                size,
                InputSizing {
                    scale_percent: 75,
                    max_edge: 0
                }
            )
            .unwrap(),
            vk::Extent2D {
                width: 1440,
                height: 810
            }
        );
        assert_eq!(
            resized(
                size,
                InputSizing {
                    scale_percent: 50,
                    max_edge: 1000
                }
            )
            .unwrap(),
            vk::Extent2D {
                width: 1000,
                height: 562
            }
        );
        assert_eq!(
            resized(
                size,
                InputSizing {
                    scale_percent: 50,
                    max_edge: 8192
                }
            )
            .unwrap(),
            size
        );
    }
    #[test]
    fn rejects_invalid_and_too_small_inputs() {
        let size = vk::Extent2D {
            width: 640,
            height: 360,
        };
        assert!(resized(
            size,
            InputSizing {
                scale_percent: 49,
                max_edge: 0
            }
        )
        .is_err());
        assert!(resized(
            size,
            InputSizing {
                scale_percent: 100,
                max_edge: 319
            }
        )
        .is_err());
        assert!(resized(
            vk::Extent2D {
                width: 640,
                height: 200
            },
            InputSizing {
                scale_percent: 50,
                max_edge: 0
            }
        )
        .is_err());
        assert!(resized(
            vk::Extent2D {
                width: 0,
                height: 0
            },
            InputSizing::default()
        )
        .is_err());
    }
}
