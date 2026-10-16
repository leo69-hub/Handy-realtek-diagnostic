//! Windows-only diagnostic policy. The pure planner is also built in tests.
use cpal::{SampleFormat, SupportedStreamConfig, SupportedStreamConfigRange};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Policy {
    Auto,
    Default,
    I16,
    Legacy,
}

impl Policy {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "default" => Ok(Self::Default),
            "i16" => Ok(Self::I16),
            "legacy" => Ok(Self::Legacy),
            _ => Err(format!(
                "Invalid HANDY_MIC_CONFIG={value:?}; use auto, default, i16, or legacy"
            )),
        }
    }
}

fn candidates(
    default: SupportedStreamConfig,
    ranges: Vec<SupportedStreamConfigRange>,
    policy: Policy,
) -> Result<Vec<SupportedStreamConfig>, String> {
    let rate = default.sample_rate();
    let mut native: Vec<_> = ranges
        .into_iter()
        .filter(|r| r.min_sample_rate() <= rate && r.max_sample_rate() >= rate)
        .map(|r| r.with_sample_rate(rate))
        .collect();
    // Try the native channel count first; retain other advertised channel counts.
    native.sort_by_key(|c| c.channels() != default.channels());
    let mut result = Vec::new();
    if matches!(policy, Policy::Auto | Policy::Default)
        || (policy == Policy::I16 && default.sample_format() == SampleFormat::I16)
    {
        result.push(default.clone());
    }
    if matches!(policy, Policy::Auto | Policy::I16) {
        result.extend(
            native
                .iter()
                .filter(|c| c.sample_format() == SampleFormat::I16)
                .cloned(),
        );
    }
    // Default and an enumerated I16 entry can describe the same stream.
    // CPAL's build_stream uses BufferSize::Default for all of these entries.
    let mut unique: Vec<SupportedStreamConfig> = Vec::new();
    for config in result {
        if !unique.iter().any(|c| {
            c.channels() == config.channels()
                && c.sample_rate() == config.sample_rate()
                && c.sample_format() == config.sample_format()
        }) {
            unique.push(config);
        }
    }
    if unique.is_empty() {
        return Err("No advertised I16 input configuration at the device default sample rate; strict i16 mode will not substitute F32".into());
    }
    Ok(unique)
}

#[cfg(target_os = "windows")]
pub(super) fn device_candidates(device: &cpal::Device) -> Result<Vec<SupportedStreamConfig>, String> {
    use cpal::traits::DeviceTrait;

    let value = std::env::var("HANDY_MIC_CONFIG").unwrap_or_else(|_| "auto".into());
    let policy = Policy::parse(&value)?;
    log::info!("Realtek diagnostic patch v1: HANDY_MIC_CONFIG={value}");
    if policy == Policy::Legacy {
        return super::AudioRecorder::get_preferred_config(device)
            .map(|c| vec![c])
            .map_err(|e| format!("Failed to fetch preferred config: {e}"));
    }
    let default = device
        .default_input_config()
        .map_err(|e| format!("Failed to fetch preferred config (device default): {e}"))?;
    log::info!("Device default input configuration: {:?}", default);
    let ranges = if policy == Policy::Default {
        Vec::new()
    } else {
        match device.supported_input_configs() {
            Ok(ranges) => ranges
                .inspect(|r| log::info!("Advertised input configuration: {:?}", r))
                .collect(),
            Err(e)
                if policy == Policy::Auto
                    || (policy == Policy::I16 && default.sample_format() == SampleFormat::I16) =>
            {
                log::warn!("Input configuration enumeration failed ({e}); trying device default only");
                Vec::new()
            }
            Err(e) => return Err(format!("Cannot enumerate I16 input configurations: {e}")),
        }
    };
    candidates(default, ranges, policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::{SampleRate, SupportedBufferSize};

    fn config(channels: u16, format: SampleFormat) -> SupportedStreamConfig {
        SupportedStreamConfig::new(
            channels,
            SampleRate(48000),
            SupportedBufferSize::Unknown,
            format,
        )
    }

    fn range(channels: u16, rate: u32, format: SampleFormat) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            SampleRate(rate),
            SampleRate(rate),
            SupportedBufferSize::Unknown,
            format,
        )
    }

    #[test]
    fn auto_preserves_exact_default_then_tries_native_i16_before_other_channels() {
        let configs = candidates(
            config(2, SampleFormat::F32),
            vec![
                range(1, 48000, SampleFormat::I16),
                range(2, 16000, SampleFormat::I16),
                range(2, 48000, SampleFormat::I16),
            ],
            Policy::Auto,
        )
        .unwrap();
        assert_eq!(configs.len(), 3);
        assert_eq!(configs[0].sample_format(), SampleFormat::F32);
        assert_eq!(configs[1].sample_format(), SampleFormat::I16);
        assert_eq!(configs[1].channels(), 2);
        assert_eq!(configs[2].channels(), 1);
        assert!(configs.iter().all(|c| c.sample_rate() == SampleRate(48000)));
    }

    #[test]
    fn i16_default_is_not_attempted_twice() {
        let configs = candidates(
            config(2, SampleFormat::I16),
            vec![range(2, 48000, SampleFormat::I16)],
            Policy::Auto,
        )
        .unwrap();
        assert_eq!(configs.len(), 1);
    }

    #[test]
    fn strict_i16_accepts_an_i16_default_even_without_enumerated_ranges() {
        let configs = candidates(config(2, SampleFormat::I16), vec![], Policy::I16).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].sample_format(), SampleFormat::I16);
    }

    #[test]
    fn strict_i16_does_not_silently_fall_back_to_float() {
        assert!(candidates(
            config(2, SampleFormat::F32),
            vec![range(2, 48000, SampleFormat::F32)],
            Policy::I16,
        )
        .is_err());
    }

    #[test]
    fn default_mode_ignores_other_formats() {
        let configs = candidates(
            config(2, SampleFormat::F32),
            vec![range(2, 48000, SampleFormat::I16)],
            Policy::Default,
        )
        .unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].sample_format(), SampleFormat::F32);
    }

    #[test]
    fn invalid_policy_is_an_error() {
        assert!(Policy::parse("f32-typo").is_err());
        assert_eq!(Policy::parse("legacy").unwrap(), Policy::Legacy);
    }
}
