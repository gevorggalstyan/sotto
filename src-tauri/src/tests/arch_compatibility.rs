#[cfg(test)]
mod tests {
    use std::env::consts::ARCH;

    #[test]
    fn test_architecture_detection() {
        let arch = ARCH;
        println!("Current architecture: {}", arch);
        assert!(arch == "x86_64" || arch == "aarch64", "Unsupported architecture");
    }

    #[test]
    fn test_metal_support() {
        if cfg!(target_os = "macos") {
            // Test Metal GPU detection
            use metal::{Device, MTLFeatureSet};
            let device = Device::system_default().expect("No Metal device found");
            println!("Metal device name: {}", device.name());

            // Verify minimum Metal feature set support
            assert!(device.supports_feature_set(MTLFeatureSet::macOS_GPUFamily1_v1),
                "Device does not support minimum required Metal feature set");
        }
    }

    #[test]
    fn test_thread_count_optimization() {
        let thread_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        println!("Available threads: {}", thread_count);
        assert!(thread_count >= 1, "Invalid thread count");

        // Test that we never exceed system thread count
        if cfg!(target_arch = "x86_64") {
            assert!(thread_count <= 32, "Excessive thread count for x86_64");
        }
    }

    #[test]
    fn test_audio_sample_rates() {
        use cpal::traits::{HostTrait, DeviceTrait};

        let host = cpal::default_host();
        if let Some(device) = host.default_input_device() {
            if let Ok(configs) = device.supported_input_configs() {
                let mut supports_16khz = false;
                let mut supports_48khz = false;

                for config in configs {
                    if config.min_sample_rate().0 <= 16000 && config.max_sample_rate().0 >= 16000 {
                        supports_16khz = true;
                    }
                    if config.min_sample_rate().0 <= 48000 && config.max_sample_rate().0 >= 48000 {
                        supports_48khz = true;
                    }
                }

                // We should support either 16kHz directly or 48kHz for downsampling
                assert!(supports_16khz || supports_48khz,
                    "No supported sample rate found (need 16kHz or 48kHz)");
            }
        }
    }
}