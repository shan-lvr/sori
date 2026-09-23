//! Audio helpers: resampling to 16 kHz mono, WAV encoding, level metering.

pub const TARGET_RATE: u32 = 16_000;

/// Resample mono `f32` samples from `in_rate` to 16 kHz.
///
/// Windowed-sinc low-pass (anti-aliasing) followed by linear interpolation. Plenty for
/// speech recognition and dependency-free.
pub fn resample_to_16k(input: &[f32], in_rate: u32) -> Vec<f32> {
    if input.is_empty() || in_rate == 0 {
        return vec![];
    }
    if in_rate == TARGET_RATE {
        return input.to_vec();
    }
    let ratio = in_rate as f64 / TARGET_RATE as f64;
    let filtered = if ratio > 1.0 {
        // Cut off a little below the new Nyquist (8 kHz).
        let cutoff = 0.45 / ratio; // normalized to input rate (cycles/sample)
        lowpass(input, cutoff, 63)
    } else {
        input.to_vec()
    };
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let a = filtered[idx.min(filtered.len() - 1)];
        let b = filtered[(idx + 1).min(filtered.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn lowpass(input: &[f32], cutoff: f64, taps: usize) -> Vec<f32> {
    let m = taps as isize / 2;
    let mut kernel = Vec::with_capacity(taps);
    let mut sum = 0.0f64;
    for n in -m..=m {
        let x = n as f64;
        let sinc = if n == 0 {
            2.0 * cutoff
        } else {
            (2.0 * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
        };
        // Blackman window
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * (n + m) as f64 / (taps - 1) as f64).cos()
            + 0.08 * (4.0 * std::f64::consts::PI * (n + m) as f64 / (taps - 1) as f64).cos();
        let v = sinc * w;
        sum += v;
        kernel.push(v);
    }
    let kernel: Vec<f32> = kernel.into_iter().map(|v| (v / sum) as f32).collect();
    let len = input.len() as isize;
    let mut out = vec![0f32; input.len()];
    for (i, o) in out.iter_mut().enumerate() {
        let mut acc = 0f32;
        for (k, kv) in kernel.iter().enumerate() {
            let j = i as isize + k as isize - m;
            if j >= 0 && j < len {
                acc += input[j as usize] * kv;
            }
        }
        *o = acc;
    }
    out
}

/// 16-bit PCM mono WAV.
pub fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let i = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        v.extend_from_slice(&i.to_le_bytes());
    }
    v
}

/// Decode a 16-bit PCM mono WAV produced by `encode_wav` (used for Retry).
pub fn decode_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return None;
    }
    let rate = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
    let samples = bytes[44..]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / i16::MAX as f32)
        .collect();
    Some((samples, rate))
}

/// RMS → perceptual 0..1 level for the HUD waveform.
pub fn level(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
    let db = 20.0 * rms.max(1e-6).log10();
    ((db + 55.0) / 45.0).clamp(0.0, 1.0)
}

/// Boost quiet recordings (whispering, distant mic) toward a healthy peak level.
pub fn normalize(samples: &mut [f32]) {
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    if peak <= 1e-4 {
        return;
    }
    let gain = (0.8 / peak).clamp(1.0, 10.0);
    if gain > 1.05 {
        for s in samples.iter_mut() {
            *s = (*s * gain).clamp(-1.0, 1.0);
        }
    }
}

/// True if the whole clip is (almost) silence.
pub fn is_silent(samples: &[f32]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let peak_window = samples
        .chunks(1600)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .fold(0f32, f32::max);
    peak_window < 0.004
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_length_and_wav_roundtrip() {
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let out = resample_to_16k(&input, 48_000);
        assert_eq!(out.len(), 16_000);
        let wav = encode_wav(&out, 16_000);
        let (dec, rate) = decode_wav(&wav).unwrap();
        assert_eq!(rate, 16_000);
        assert_eq!(dec.len(), out.len());
    }
}
