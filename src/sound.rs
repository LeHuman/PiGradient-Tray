//! Subtle, synthesized UI ticks for flyout interactions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSound {
    Toggle,
    Action,
    Open,
    Close,
}

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;

    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    use windows::core::PCWSTR;

    use super::UiSound;

    /// 16-bit mono PCM WAV with a short, exponentially damped sine.
    fn tone(freq: f32, duration_ms: u32, amp: f32) -> Vec<u8> {
        const RATE: u32 = 44_100;
        let samples = (RATE * duration_ms / 1000) as usize;
        let data_len = (samples * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());

        for i in 0..samples {
            let t = i as f32 / RATE as f32;
            let p = i as f32 / samples.max(1) as f32;
            let attack = (p * 80.0).min(1.0);
            let env = attack * (1.0 - p).powf(2.5);
            let sample = (t * freq * std::f32::consts::TAU).sin() * env * amp;
            wav.extend_from_slice(&((sample * i16::MAX as f32) as i16).to_le_bytes());
        }
        wav
    }

    fn buffer(sound: UiSound) -> &'static [u8] {
        static TOGGLE: OnceLock<Vec<u8>> = OnceLock::new();
        static ACTION: OnceLock<Vec<u8>> = OnceLock::new();
        static OPEN: OnceLock<Vec<u8>> = OnceLock::new();
        static CLOSE: OnceLock<Vec<u8>> = OnceLock::new();
        match sound {
            UiSound::Toggle => TOGGLE.get_or_init(|| tone(1_500.0, 10, 0.06)),
            UiSound::Action => ACTION.get_or_init(|| tone(1_100.0, 12, 0.06)),
            UiSound::Open => OPEN.get_or_init(|| tone(900.0, 14, 0.055)),
            UiSound::Close => CLOSE.get_or_init(|| tone(700.0, 12, 0.055)),
        }
    }

    pub fn play(sound: UiSound) {
        let data = buffer(sound);
        // SAFETY: the WAV buffer is static for the process lifetime, as required
        // by `SND_MEMORY | SND_ASYNC`.
        unsafe {
            let _ = PlaySoundW(
                PCWSTR(data.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
            );
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn synthesizes_a_valid_mono_wav_header() {
            let wav = tone(1_000.0, 10, 0.06);
            assert!(wav.len() > 44);
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
            assert_eq!(&wav[36..40], b"data");
        }
    }
}

#[cfg(windows)]
pub use imp::play;

#[cfg(not(windows))]
pub fn play(_sound: UiSound) {}
