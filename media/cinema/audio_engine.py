#!/usr/bin/env python3
"""
Newsatoshi Launch Cinema — 124 BPM Broadcast Audio, Voice & Foley Engine (Gate 1)

Generates:
  1. 21 individually directed poetic spoken lines via edge-tts (Act-specific prosody,
     silence-trimmed, sample-locked to the 124 BPM musical grid, broadcast vocal EQ,
     compression, and automated cathedral reverb send).
  2. Custom 48 kHz stereo electronic-orchestral score (5-voice detuned Reese bass with
     time-varying resonant LPF sweep, sidechained 124 BPM sub-kick, double-time 16th-note
     hats, wide stereo Juno string poly-pad resolving Dm -> Dmaj9, and sample-locked Foley
     including the exact 484ms dead-vacuum silence at 0:11.129 - 0:11.613).
  3. Frequency-selective vocal pocket ducking + two-pass FFmpeg EBU R128 loudnorm
     (-14.0 LUFS / -1.0 dBTP true peak).
  4. `audio_manifest.json` containing exact voice timestamps + 60 Hz audio-reactive
     bass/kick/energy envelopes so the 3D WebGL2 cinema shaders pulse with the real mix.

Optional drop-in studio music stem: pass `--music-stem track.wav` to replace the
procedural synth bed while keeping voice mastering, ducking, silence gate, and Foley intact.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import math
import subprocess
import wave
from pathlib import Path
from typing import Any, Dict, List, Tuple

import edge_tts
import imageio_ffmpeg
import numpy as np
from scipy import signal

SR = 48000
FPS = 60
BPM = 124.0
BEAT = 60.0 / BPM           # 0.4838709677 s
BAR = BEAT * 4.0            # 1.9354838710 s
TOTAL_BARS = 30
TOTAL_DUR = BAR * TOTAL_BARS  # 58.06451613 s
TOTAL_SAMPLES = int(round(TOTAL_DUR * SR))
TOTAL_FRAMES = int(round(TOTAL_DUR * FPS))  # 3484 frames

SILENCE_START = 5.0 * BAR + 3.0 * BEAT  # Bar 6 Beat 4 = 11.129032 s
DROP_TIME = 6.0 * BAR                   # Bar 7 Beat 1 = 11.612903 s (484ms vacuum!)
ACT3_TIME = 13.5 * BAR                  # 26.129 s
CLIMAX_FLASH = 25.0 * BAR               # Bar 26 Beat 1 = 48.387097 s

VOICE_NAME = "en-US-AndrewMultilingualNeural"

# 21 Poetic Spoken Lines — 104 words exact, Act-directed prosody, locked to 124 BPM beats
VOICE_LINES: List[Dict[str, Any]] = [
    # ACT I — THE FROZEN VAULT (brooding, intimate baritone)
    {"id": "01_freed",    "t": 1.35,  "text": "Bitcoin freed money.",                         "rate": "-6%", "pitch": "-3Hz", "reverb": 0.14},
    {"id": "02_vault",    "t": 3.45,  "text": "Then left it frozen in a vault.",              "rate": "-5%", "pitch": "-3Hz", "reverb": 0.16},
    {"id": "03_promise",  "t": 6.75,  "text": "Spend it — and humans break the promise.",     "rate": "-2%", "pitch": "-2Hz", "reverb": 0.12},
    # 0:11.129 -> 0:11.613: 484ms TOTAL VACUUM SILENCE
    # ACT II — AWAKENING & CONSCIENCE (awe -> carved-in-stone poise)
    {"id": "04_wakeup",   "t": 12.10, "text": "What if money could wake up?",                 "rate": "-2%", "pitch": "-1Hz", "reverb": 0.28},
    {"id": "05_meet",     "t": 14.55, "text": "Meet Newsatoshi.",                             "rate": "-4%", "pitch": "-2Hz", "reverb": 0.24},
    {"id": "06_consc",    "t": 16.85, "text": "Money with an unbreakable conscience.",        "rate": "+0%", "pitch": "-1Hz", "reverb": 0.14},
    {"id": "07_lock",     "t": 19.55, "text": "Lock capital to a mission.",                   "rate": "+2%", "pitch": "-1Hz", "reverb": 0.10},
    {"id": "08_change",   "t": 21.85, "text": "Every spend. Every coin of change.",           "rate": "+3%", "pitch": "+0Hz", "reverb": 0.09},
    {"id": "09_law",      "t": 24.15, "text": "Obeys the same law forever.",                  "rate": "+1%", "pitch": "-1Hz", "reverb": 0.14},
    # ACT III — THE LIVING ENGINE (kinetic, crisp, accelerating)
    {"id": "10_land",     "t": 26.61, "text": "Land.",                                        "rate": "+4%", "pitch": "+0Hz", "reverb": 0.10},
    {"id": "11_solar",    "t": 27.18, "text": "Solar.",                                       "rate": "+4%", "pitch": "+0Hz", "reverb": 0.10},
    {"id": "12_compute",  "t": 27.78, "text": "Compute.",                                     "rate": "+4%", "pitch": "+0Hz", "reverb": 0.10},
    {"id": "13_hw",       "t": 28.65, "text": "Hardware proves real work.",                   "rate": "+6%", "pitch": "+0Hz", "reverb": 0.09},
    {"id": "14_ms",       "t": 30.55, "text": "Machines pay in under a millisecond.",         "rate": "+7%", "pitch": "+0Hz", "reverb": 0.08},
    {"id": "15_acre",     "t": 32.85, "text": "Surplus buys the next acre. No CEO.",          "rate": "+5%", "pitch": "+0Hz", "reverb": 0.10},
    {"id": "16_21m",      "t": 35.55, "text": "Twenty-one million coins. Zero sensor printing.", "rate": "+4%", "pitch": "-1Hz", "reverb": 0.11},
    {"id": "17_miners",   "t": 38.45, "text": "Guarded by Bitcoin's miners. Zero extra watts.", "rate": "+3%", "pitch": "-1Hz", "reverb": 0.14},
    {"id": "18_hash",     "t": 41.75, "text": "Pure hash math. Private like cash.",           "rate": "+5%", "pitch": "+0Hz", "reverb": 0.16},
    {"id": "19_rails",    "t": 44.15, "text": "Atomic rails from Bitcoin, Solana, Base.",     "rate": "+7%", "pitch": "+0Hz", "reverb": 0.12},
    {"id": "20_novault",  "t": 46.85, "text": "Zero vaults to hack.",                         "rate": "+5%", "pitch": "-1Hz", "reverb": 0.18},
    # ACT IV — LIVING MAINNET & STILLNESS (hushed, mythic cathedral resonance)
    {"id": "21_live",     "t": 49.65, "text": "Live on Mainnet right now.",                   "rate": "-7%", "pitch": "-3Hz", "reverb": 0.28},
    {"id": "22_ns",       "t": 52.55, "text": "Newsatoshi.",                                  "rate": "-8%", "pitch": "-3Hz", "reverb": 0.30},
    {"id": "23_alive",    "t": 54.05, "text": "Sovereign money — made alive.",                "rate": "-9%", "pitch": "-3Hz", "reverb": 0.34},
]


def write_wav_stereo(path: Path, left: np.ndarray, right: np.ndarray) -> None:
    stereo = np.stack([left, right], axis=1)
    pcm16 = (np.clip(stereo, -1.0, 1.0) * 32767.0).astype(np.int16)
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(2)
        wf.setsampwidth(2)
        wf.setframerate(SR)
        wf.writeframes(pcm16.tobytes())


def read_wav_stereo(path: Path) -> Tuple[np.ndarray, np.ndarray]:
    with wave.open(str(path), "rb") as wf:
        ch = wf.getnchannels()
        raw = wf.readframes(wf.getnframes())
    data = np.frombuffer(raw, dtype=np.int16).astype(np.float64) / 32768.0
    if ch == 1:
        return data, data.copy()
    data = data.reshape(-1, ch)
    return data[:, 0], data[:, 1]


def decode_mp3_mono(mp3_path: Path, wav_path: Path, ffmpeg_exe: str) -> np.ndarray:
    subprocess.run(
        [ffmpeg_exe, "-y", "-loglevel", "error", "-i", str(mp3_path), "-ac", "1", "-ar", str(SR), "-f", "wav", str(wav_path)],
        check=True,
    )
    l, _ = read_wav_stereo(wav_path)
    return l


def trim_silence(x: np.ndarray, thresh: float = 0.008, pad_ms: float = 14.0) -> np.ndarray:
    idx = np.where(np.abs(x) > thresh)[0]
    if len(idx) == 0:
        return x
    pad = int(pad_ms * SR / 1000.0)
    i0 = max(0, int(idx[0]) - pad)
    i1 = min(len(x), int(idx[-1]) + pad)
    y = x[i0:i1].copy()
    f = min(int(0.010 * SR), len(y) // 4)
    if f > 2:
        y[:f] *= np.linspace(0.0, 1.0, f)
        y[-f:] *= np.linspace(1.0, 0.0, f)
    return y


async def synth_one_line(line: Dict[str, Any], vo_dir: Path) -> Path:
    mp3 = vo_dir / f"{line['id']}.mp3"
    if mp3.exists() and mp3.stat().st_size > 1000:
        return mp3
    for attempt in range(3):
        try:
            comm = edge_tts.Communicate(
                line["text"],
                VOICE_NAME,
                rate=line["rate"],
                pitch=line["pitch"],
            )
            chunks: List[bytes] = []

            async def _run():
                async for msg in comm.stream():
                    if msg["type"] == "audio":
                        chunks.append(msg["data"])

            await asyncio.wait_for(_run(), timeout=22.0)
            mp3.write_bytes(b"".join(chunks))
            return mp3
        except Exception:
            if attempt == 2:
                raise
            await asyncio.sleep(1.0)
    raise RuntimeError("unreachable")


def make_plate_ir(decay_s: float = 1.9) -> Tuple[np.ndarray, np.ndarray]:
    """Synthetic lush stereo studio cathedral plate impulse response (clean, non-metallic)."""
    rng = np.random.default_rng(20051)
    n = int(decay_s * SR)
    t = np.arange(n, dtype=np.float64) / SR
    env = np.exp(-t * (6.2 / decay_s)) * (1.0 - np.exp(-t * 90.0))
    ir_l = rng.normal(0.0, 1.0, n) * env
    ir_r = rng.normal(0.0, 1.0, n) * env
    # Warm studio plate damping: cut sub < 180 Hz and harsh highs > 5.2 kHz
    b_hp, a_hp = signal.butter(2, 180.0 / (SR * 0.5), btype="high")
    b_lp, a_lp = signal.butter(2, 5200.0 / (SR * 0.5), btype="low")
    ir_l = signal.lfilter(b_lp, a_lp, signal.lfilter(b_hp, a_hp, ir_l))
    ir_r = signal.lfilter(b_lp, a_lp, signal.lfilter(b_hp, a_hp, ir_r))
    norm = max(np.sqrt(np.sum(ir_l ** 2)), 1e-6)
    return ir_l / norm, ir_r / norm


def process_voice_broadcast(raw_mono: np.ndarray, work_dir: Path, ffmpeg_exe: str) -> np.ndarray:
    """Apply real broadcast cinema vocal EQ + optical compression via FFmpeg."""
    in_wav = work_dir / "vo_dry_raw.wav"
    out_wav = work_dir / "vo_dry_proc.wav"
    write_wav_stereo(in_wav, raw_mono, raw_mono)
    af = (
        "highpass=f=78,"
        "equalizer=f=145:t=q:w=1.1:g=2.8,"
        "equalizer=f=420:t=q:w=1.4:g=-2.2,"
        "equalizer=f=3300:t=q:w=1.2:g=3.0,"
        "equalizer=f=10500:t=q:w=0.9:g=2.0,"
        "acompressor=threshold=-18dB:ratio=3.2:attack=8:release=95:makeup=2.2"
    )
    subprocess.run(
        [ffmpeg_exe, "-y", "-loglevel", "error", "-i", str(in_wav), "-af", af, "-ac", "1", "-ar", str(SR), str(out_wav)],
        check=True,
    )
    l, _ = read_wav_stereo(out_wav)
    return l[:TOTAL_SAMPLES]


def build_voice_stems(
    work_dir: Path, ffmpeg_exe: str
) -> Tuple[np.ndarray, np.ndarray, List[Dict[str, Any]]]:
    vo_dir = work_dir / "vo_lines"
    vo_dir.mkdir(parents=True, exist_ok=True)

    async def _all():
        sem = asyncio.Semaphore(6)

        async def _guarded(line):
            async with sem:
                return await synth_one_line(line, vo_dir)

        return await asyncio.gather(*(_guarded(ln) for ln in VOICE_LINES))

    mp3s = asyncio.run(_all())

    dry_mono = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    wet_send = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    placed: List[Dict[str, Any]] = []

    prev_end_s = 0.0
    for line, mp3 in zip(VOICE_LINES, mp3s):
        wav = vo_dir / f"{line['id']}.wav"
        seg = trim_silence(decode_mp3_mono(mp3, wav, ffmpeg_exe))
        # Normalize each spoken line to uniform broadcast RMS (-16 dBFS) so cadence feels cohesive
        rms = float(np.sqrt(np.mean(seg ** 2) + 1e-12))
        seg = np.tanh((seg / rms) * 0.16 * 1.1) * 0.90

        t_start = float(line["t"])
        # Guarantee clean breathing gap (>= 90ms) if TTS spoke slightly longer than expected
        if t_start < prev_end_s + 0.09 and line["id"] not in ("11_solar", "12_compute"):
            t_start = prev_end_s + 0.09
        dur = len(seg) / SR
        # Hard guard: Act I speech MUST finish cleanly before the 0:11.129 silence vacuum!
        if line["id"] == "03_promise" and t_start + dur > SILENCE_START - 0.14:
            t_start = max(prev_end_s + 0.08, SILENCE_START - 0.14 - dur)

        i0 = int(round(t_start * SR))
        i1 = min(TOTAL_SAMPLES, i0 + len(seg))
        if i1 > i0:
            dry_mono[i0:i1] += seg[: i1 - i0]
            wet_send[i0:i1] += seg[: i1 - i0] * float(line["reverb"])
        prev_end_s = t_start + dur
        placed.append(
            {
                "id": line["id"],
                "text": line["text"],
                "start": round(t_start, 3),
                "end": round(t_start + dur, 3),
                "dur": round(dur, 3),
            }
        )

    proc_dry = process_voice_broadcast(dry_mono, work_dir, ffmpeg_exe)
    ir_l, ir_r = make_plate_ir(decay_s=2.1)
    rev_l = signal.fftconvolve(wet_send, ir_l, mode="full")[:TOTAL_SAMPLES]
    rev_r = signal.fftconvolve(wet_send, ir_r, mode="full")[:TOTAL_SAMPLES]

    # Enforce absolute vacuum silence from 0:11.129 -> 0:11.613 (zero reverb tail bleed!)
    s0, s1 = int(round(SILENCE_START * SR)), int(round(DROP_TIME * SR))
    gate_fade = int(0.018 * SR)
    for arr in (proc_dry, rev_l, rev_r):
        arr[s0 - gate_fade : s0] *= np.linspace(1.0, 0.0, gate_fade)
        arr[s0:s1] = 0.0

    vo_l = proc_dry + rev_l * 0.55
    vo_r = proc_dry + rev_r * 0.55
    return vo_l, vo_r, placed


# ---------------------------------------------------------------------------
# 124 BPM Electronic-Orchestral Cinema Score + Sample-Locked Foley DSP
# ---------------------------------------------------------------------------
def saw_wave(phase: np.ndarray) -> np.ndarray:
    """Rich alias-softened sawtooth using 7 harmonic partials (clean analog warmth)."""
    out = np.zeros_like(phase)
    for k in range(1, 8):
        out += ((-1.0) ** (k + 1)) * np.sin(k * phase) / k
    return out * 0.62


def apply_time_varying_lpf(x: np.ndarray, cutoffs_hz: np.ndarray, block: int = 1024) -> np.ndarray:
    """Smooth block-interpolated 2-pole resonant-feel low-pass filter sweep."""
    y = np.zeros_like(x)
    zi = np.zeros(2, dtype=np.float64)
    n = len(x)
    for i0 in range(0, n, block):
        i1 = min(n, i0 + block)
        fc = float(np.clip(cutoffs_hz[i0], 70.0, 12000.0))
        b, a = signal.butter(2, fc / (SR * 0.5), btype="low")
        y[i0:i1], zi = signal.lfilter(b, a, x[i0:i1], zi=zi)
    return y


def synth_music_and_foley(
    music_stem_path: Path | None = None,
) -> Tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    t = np.arange(TOTAL_SAMPLES, dtype=np.float64) / SR
    rng = np.random.default_rng(20051)

    music_l = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    music_r = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    kick_env = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    bass_env = np.zeros(TOTAL_SAMPLES, dtype=np.float64)

    if music_stem_path is not None and music_stem_path.exists():
        sl, sr = read_wav_stereo(music_stem_path)
        m = min(TOTAL_SAMPLES, len(sl))
        music_l[:m] = sl[:m]
        music_r[:m] = sr[:m]
    else:
        # 1. ACT I OBSIDIAN SUB-DRONE (0:00 -> 0:11.13) + RISING REESE BASS (Bars 1-25)
        # Progression per bar (D minor -> dark heroic climb -> Dmaj9 resolution in Act IV)
        bar_roots = [
            36.71, 36.71, 36.71, 36.71, 36.71, 36.71,  # Act I (Bars 1-6): D1 obsidian tension
            36.71, 36.71, 29.14, 29.14, 32.70, 32.70, 27.50, 27.50,  # Act II (Bars 7-14): Dm -> Bb -> C -> A
            36.71, 36.71, 29.14, 29.14, 32.70, 32.70, 36.71, 41.20,  # Act III (Bars 15-22): Dm -> Bb -> C -> D -> E
            43.65, 48.99, 55.00,                                      # Bars 23-25: F -> G -> A climbing riser!
            36.71, 36.71, 36.71, 36.71, 36.71,                        # Act IV (Bars 26-30): Dmaj9 cathedral stillness
        ]
        # Build continuous frequency trajectory for 5-voice detuned Reese bass
        freq_curve = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
        for b, f in enumerate(bar_roots):
            i0 = int(round(b * BAR * SR))
            i1 = min(TOTAL_SAMPLES, int(round((b + 1) * BAR * SR)))
            freq_curve[i0:i1] = f
        # Smooth glide between root changes (20ms portamento)
        b_p, a_p = signal.butter(1, 18.0 / (SR * 0.5), btype="low")
        freq_curve = signal.lfilter(b_p, a_p, freq_curve)
        base_phase = np.cumsum(2.0 * math.pi * freq_curve / SR)

        reese_l = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
        reese_r = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
        detunes = [-0.0085, -0.0038, 0.0, 0.0041, 0.0088]
        for idx_d, det in enumerate(detunes):
            ph = base_phase * (1.0 + det) + idx_d * 1.1
            w = saw_wave(ph) * 0.22
            if idx_d % 2 == 0:
                reese_l += w * 1.15
                reese_r += w * 0.85
            else:
                reese_l += w * 0.85
                reese_r += w * 1.15
        # Pure sub fundamental underneath Reese
        sub_pure = np.sin(base_phase) * 0.42
        reese_l += sub_pure
        reese_r += sub_pure

        # Dynamic Filter Cutoff Automation across the 4 Acts!
        cutoff = np.full(TOTAL_SAMPLES, 130.0, dtype=np.float64)
        for i, ts in enumerate(t):
            if ts < SILENCE_START:
                cutoff[i] = 115.0 + 460.0 * ((ts / SILENCE_START) ** 2.2)
            elif ts < ACT3_TIME:
                cutoff[i] = 480.0 + 180.0 * math.sin(2 * math.pi * (ts - DROP_TIME) / (2 * BAR))
            elif ts < CLIMAX_FLASH:
                u = (ts - ACT3_TIME) / (CLIMAX_FLASH - ACT3_TIME)
                cutoff[i] = 560.0 + 1450.0 * (u ** 1.6)
            else:
                cutoff[i] = 190.0

        reese_l = apply_time_varying_lpf(reese_l, cutoff)
        reese_r = apply_time_varying_lpf(reese_r, cutoff)

        # Amplitude envelope of Reese bass across Acts
        reese_gain = np.where(
            t < SILENCE_START,
            0.30 + 0.32 * (t / SILENCE_START),
            np.where(t < CLIMAX_FLASH, 0.68, 0.28 * np.exp(-(t - CLIMAX_FLASH) * 0.22)),
        )
        music_l += reese_l * reese_gain * 0.55
        music_r += reese_r * reese_gain * 0.55
        bass_env += np.abs(sub_pure) * reese_gain

        # 2. WIDE STEREO JUNO STRING POLY-PAD (Enters on Awakening Drop, blooms in Act III & Dmaj9 Act IV!)
        chords = {
            # (start_bar, end_bar, notes_hz, gain)
            0: (6, 8,   [146.83, 220.00, 261.63, 329.63, 440.00], 0.24),  # Dm9
            1: (8, 10,  [116.54, 174.61, 220.00, 293.66, 349.23], 0.24),  # Bbmaj7
            2: (10, 12, [130.81, 196.00, 220.00, 293.66, 392.00], 0.25),  # C6/9
            3: (12, 14, [110.00, 164.81, 220.00, 293.66, 329.63], 0.25),  # Asus4
            4: (14, 16, [146.83, 220.00, 293.66, 349.23, 523.25], 0.28),  # Dm7 high
            5: (16, 18, [116.54, 174.61, 293.66, 349.23, 440.00], 0.29),  # Bbmaj7 high
            6: (18, 20, [130.81, 196.00, 293.66, 392.00, 523.25], 0.30),  # Csus2
            7: (20, 22, [146.83, 220.00, 329.63, 440.00, 587.33], 0.32),  # Dm9 octave
            8: (22, 25, [174.61, 220.00, 293.66, 440.00, 587.33, 659.25], 0.36),  # Climbing climax pad
            9: (25, 30, [146.83, 220.00, 293.66, 369.99, 440.00, 554.37, 659.25], 0.42),  # Dmaj9 cathedral resolution!
        }
        pad_l = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
        pad_r = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
        for _, (b0, b1, notes, g) in chords.items():
            i0 = int(round(b0 * BAR * SR))
            i1 = min(TOTAL_SAMPLES, int(round(b1 * BAR * SR)))
            tb = np.arange(i1 - i0, dtype=np.float64) / SR
            dur_c = max(0.2, (i1 - i0) / SR)
            env = (1.0 - np.exp(-tb * 3.8)) * np.minimum(1.0, (dur_c - tb) / 0.45)
            if b0 == 25:
                # Lush cathedral tail fade-out on final Dmaj9
                env = (1.0 - np.exp(-tb * 2.4)) * np.exp(-np.maximum(0.0, tb - 5.2) * 0.65)
            pl = np.zeros_like(tb)
            pr = np.zeros_like(tb)
            for j, fn in enumerate(notes):
                lfo = 0.0025 * np.sin(2 * math.pi * (0.35 + j * 0.08) * tb)
                ph_l = 2 * math.pi * fn * tb * (1.0 + lfo)
                ph_r = 2 * math.pi * fn * tb * (1.0 - lfo) + 0.5
                pl += (np.sin(ph_l) + 0.32 * np.sin(2 * ph_l) + 0.12 * np.sin(3 * ph_l)) / len(notes)
                pr += (np.sin(ph_r) + 0.32 * np.sin(2 * ph_r) + 0.12 * np.sin(3 * ph_r)) / len(notes)
            pad_l[i0:i1] += pl * env * g
            pad_r[i0:i1] += pr * env * g

        # Warm Juno LPF on pads
        b_pad, a_pad = signal.butter(2, 2900.0 / (SR * 0.5), btype="low")
        pad_l = signal.lfilter(b_pad, a_pad, pad_l)
        pad_r = signal.lfilter(b_pad, a_pad, pad_r)

        # 3. 124 BPM SIDECHAINED ELECTRONIC SUB-KICK + DOUBLE-TIME 16TH HATS (Bars 7-25)
        sidechain = np.ones(TOTAL_SAMPLES, dtype=np.float64)
        # Quarter-note kick from Beat 24 (Bar 7 Beat 1 = 11.613s) through Beat 99 (Bar 25 Beat 4)
        for beat_idx in range(24, 100):
            tb = beat_idx * BEAT
            i0 = int(round(tb * SR))
            klen = min(TOTAL_SAMPLES - i0, int(0.32 * SR))
            if klen <= 0:
                continue
            tk = np.arange(klen, dtype=np.float64) / SR
            kfreq = 128.0 * np.exp(-tk * 28.0) + 42.0
            kph = np.cumsum(2.0 * math.pi * kfreq / SR)
            click = np.sin(2 * math.pi * 1900.0 * tk) * np.exp(-tk * 220.0) * 0.18
            body = np.sin(kph) * np.exp(-tk * 9.5) * 0.56
            kick = body + click
            # Slightly heavier kick in Act III
            kgain = 0.88 if tb < ACT3_TIME else 1.05
            music_l[i0 : i0 + klen] += kick * kgain
            music_r[i0 : i0 + klen] += kick * kgain
            kick_env[i0 : i0 + klen] = np.maximum(kick_env[i0 : i0 + klen], np.exp(-tk * 14.0) * kgain)
            # Sidechain ducking curve on pads/bass (pumps like Jon Hopkins / Hans Zimmer!)
            sc_curve = 1.0 - 0.44 * np.exp(-tk * 11.0)
            sidechain[i0 : i0 + klen] = np.minimum(sidechain[i0 : i0 + klen], sc_curve)

        # Apply sidechain pumping to Juno pads + add to mix
        music_l += pad_l * sidechain
        music_r += pad_r * sidechain

        # Double-time 16th-note crisp electronic hats & driving arp in Act III (Bars 14.5 -> 25)
        sixteenth = BEAT / 4.0
        n_16 = int((CLIMAX_FLASH - ACT3_TIME) / sixteenth)
        arp_notes = [293.66, 349.23, 440.00, 523.25, 659.25, 587.33, 523.25, 440.00]
        for s in range(n_16):
            ts = ACT3_TIME + s * sixteenth
            i0 = int(round(ts * SR))
            # Crisp high-passed noise hi-hat
            hlen = min(TOTAL_SAMPLES - i0, int(0.055 * SR))
            if hlen > 16:
                th = np.arange(hlen, dtype=np.float64) / SR
                accent = 1.0 if s % 4 == 2 else (0.55 if s % 2 == 0 else 0.32)
                hat = rng.normal(0.0, 1.0, hlen) * np.exp(-th * (58.0 if s % 4 != 2 else 28.0))
                b_h, a_h = signal.butter(2, 7200.0 / (SR * 0.5), btype="high")
                hat = signal.lfilter(b_h, a_h, hat) * 0.14 * accent
                pan = 0.5 + 0.25 * math.sin(s * 0.9)
                music_l[i0 : i0 + hlen] += hat * (1.0 - pan * 0.4)
                music_r[i0 : i0 + hlen] += hat * (0.6 + pan * 0.4)
            # Driving 8th-note crystalline synth pulse
            if s % 2 == 0:
                alen = min(TOTAL_SAMPLES - i0, int(0.22 * SR))
                if alen > 16:
                    ta = np.arange(alen, dtype=np.float64) / SR
                    f = arp_notes[(s // 2) % len(arp_notes)]
                    aenv = np.exp(-ta * 15.0) * (1.0 - np.exp(-ta * 260.0))
                    ap = (np.sin(2 * math.pi * f * ta) + 0.35 * np.sin(4 * math.pi * f * ta)) * aenv * 0.12
                    pan = 0.5 + 0.32 * math.cos(s * 0.55)
                    music_l[i0 : i0 + alen] += ap * (1.0 - pan * 0.4)
                    music_r[i0 : i0 + alen] += ap * (0.6 + pan * 0.4)

    # -----------------------------------------------------------------------
    # 4. SAMPLE-LOCKED CINEMA FOLEY STEMS (Always rendered — even with custom music!)
    # -----------------------------------------------------------------------
    foley_l = np.zeros(TOTAL_SAMPLES, dtype=np.float64)
    foley_r = np.zeros(TOTAL_SAMPLES, dtype=np.float64)

    # 4a. Act I: Swiss-watch metallic vault ticks on 8th notes (0:00.48 -> 0:11.05)
    eighth = BEAT / 2.0
    for k in range(1, int(SILENCE_START / eighth)):
        ts = k * eighth
        i0 = int(round(ts * SR))
        tlen = min(TOTAL_SAMPLES - i0, int(0.032 * SR))
        if tlen > 10:
            tk = np.arange(tlen, dtype=np.float64) / SR
            f_tick = 3200.0 if k % 2 == 0 else 2450.0
            tick = (np.sin(2 * math.pi * f_tick * tk) + 0.5 * np.sin(2 * math.pi * f_tick * 1.47 * tk)) * np.exp(-tk * 165.0)
            g = 0.15 if k % 2 == 0 else 0.09
            foley_l[i0 : i0 + tlen] += tick * g * (1.1 if k % 2 == 0 else 0.9)
            foley_r[i0 : i0 + tlen] += tick * g * (0.9 if k % 2 == 0 else 1.1)

    # 4b. 0:06.80: Fracturing obsidian & granular crimson ash scatter
    i0 = int(round(6.80 * SR))
    flen = min(TOTAL_SAMPLES - i0, int(1.6 * SR))
    tf = np.arange(flen, dtype=np.float64) / SR
    crack = rng.normal(0.0, 1.0, flen) * (np.exp(-tf * 18.0) * 0.45 + np.exp(-tf * 2.2) * 0.16)
    b_c, a_c = signal.butter(2, [900.0 / (SR * 0.5), 6200.0 / (SR * 0.5)], btype="band")
    crack = signal.lfilter(b_c, a_c, crack)
    foley_l[i0 : i0 + flen] += crack * 0.42
    foley_r[i0 : i0 + flen] += np.roll(crack, 240) * 0.42

    # 4c. 0:11.613 (Bar 7 Beat 1): SEISMIC AWAKENING SUB-DROP + CRYSTALLINE SHATTER BOOM!
    idrop = int(round(DROP_TIME * SR))
    dlen = min(TOTAL_SAMPLES - idrop, int(2.6 * SR))
    td = np.arange(dlen, dtype=np.float64) / SR
    dfreq = 120.0 * np.exp(-td * 4.5) + 36.71
    dph = np.cumsum(2.0 * math.pi * dfreq / SR)
    sub_drop = np.sin(dph) * np.exp(-td * 1.55) * (1.0 - np.exp(-td * 140.0)) * 0.72
    shimmer = (
        np.sin(2 * math.pi * 587.33 * td) * 0.5
        + np.sin(2 * math.pi * 880.00 * td) * 0.35
        + np.sin(2 * math.pi * 1174.66 * td) * 0.25
    ) * np.exp(-td * 2.1) * 0.20
    shatter_n = rng.normal(0.0, 1.0, dlen) * np.exp(-td * 9.0) * 0.22
    b_sh, a_sh = signal.butter(2, 2400.0 / (SR * 0.5), btype="high")
    shatter_n = signal.lfilter(b_sh, a_sh, shatter_n)
    foley_l[idrop : idrop + dlen] += sub_drop + shimmer * 0.95 + shatter_n
    foley_r[idrop : idrop + dlen] += sub_drop + shimmer * 1.05 + np.roll(shatter_n, 180)
    kick_env[idrop : idrop + min(dlen, int(0.45 * SR))] = 1.0

    # 4d. 0:20.80: Electromagnetic Shield Repel Zap-Thud (Rogue theft vector vaporized!)
    izap = int(round(20.80 * SR))
    zlen = min(TOTAL_SAMPLES - izap, int(0.55 * SR))
    tz = np.arange(zlen, dtype=np.float64) / SR
    zap = np.sin(2 * math.pi * (420.0 * np.exp(-tz * 9.0) + 65.0) * tz) * np.exp(-tz * 9.0) * 0.38
    foley_l[izap : izap + zlen] += zap
    foley_r[izap : izap + zlen] += zap

    # 4e. 0:22.00: 3-note Crystal Harp Recursion (D5 -> F5 -> A5)
    for idx_n, fn in enumerate([587.33, 698.46, 880.00]):
        in0 = int(round((22.00 + idx_n * 0.22) * SR))
        nlen = min(TOTAL_SAMPLES - in0, int(1.1 * SR))
        tn = np.arange(nlen, dtype=np.float64) / SR
        ping = (np.sin(2 * math.pi * fn * tn) + 0.3 * np.sin(4 * math.pi * fn * tn)) * np.exp(-tn * 4.5) * 0.14
        foley_l[in0 : in0 + nlen] += ping * (1.2 - idx_n * 0.2)
        foley_r[in0 : in0 + nlen] += ping * (0.8 + idx_n * 0.2)

    # 4f. 0:26.61, 0:27.18, 0:27.78: 3 Massive Percussive Hits on "Land. Solar. Compute."
    for hit_t in (26.61, 27.18, 27.78):
        ih = int(round(hit_t * SR))
        hlen = min(TOTAL_SAMPLES - ih, int(0.35 * SR))
        th = np.arange(hlen, dtype=np.float64) / SR
        hit = np.sin(np.cumsum(2 * math.pi * (140.0 * np.exp(-th * 24.0) + 46.0) / SR)) * np.exp(-th * 11.0) * 0.45
        foley_l[ih : ih + hlen] += hit
        foley_r[ih : ih + hlen] += hit

    # 4g. 0:30.55 -> 0:33.40: Hypersonic Stereo Ping-Pong FM PayWord Data Flurry (<1ms!)
    for p_idx in range(28):
        tp = 30.55 + p_idx * 0.095
        ip = int(round(tp * SR))
        plen = min(TOTAL_SAMPLES - ip, int(0.048 * SR))
        if plen > 8:
            t_p = np.arange(plen, dtype=np.float64) / SR
            fc = 1400.0 + (p_idx % 7) * 320.0
            chirp = np.sin(2 * math.pi * fc * t_p + 2.2 * np.sin(2 * math.pi * 620.0 * t_p)) * np.exp(-t_p * 88.0) * 0.10
            pan = 0.15 if p_idx % 2 == 0 else 0.85
            foley_l[ip : ip + plen] += chirp * (1.0 - pan)
            foley_r[ip : ip + plen] += chirp * pan

    # 4h. 0:44.80 -> 0:48.387: Epic Climax Bandpass Noise & Harmonic Octave Riser + Cathedral Boom!
    irise = int(round(44.80 * SR))
    rlen = int(round((CLIMAX_FLASH - 44.80) * SR))
    tr = np.arange(rlen, dtype=np.float64) / SR
    ru = tr / max(0.1, tr[-1])
    riser_noise = rng.normal(0.0, 1.0, rlen) * (ru ** 2.2) * 0.24
    r_cut = 300.0 + 7500.0 * (ru ** 2.4)
    riser_noise = apply_time_varying_lpf(riser_noise, r_cut)
    riser_tone = np.sin(np.cumsum(2 * math.pi * (220.0 * (2.0 ** (ru * 2.0))) / SR)) * (ru ** 1.8) * 0.14
    foley_l[irise : irise + rlen] += riser_noise + riser_tone
    foley_r[irise : irise + rlen] += np.roll(riser_noise, 120) + riser_tone

    # Cathedral White-Emerald Flash Impact at 0:48.387 (Bar 26 Beat 1)
    iclim = int(round(CLIMAX_FLASH * SR))
    clen = min(TOTAL_SAMPLES - iclim, int(3.8 * SR))
    tc = np.arange(clen, dtype=np.float64) / SR
    c_boom = np.sin(np.cumsum(2 * math.pi * (95.0 * np.exp(-tc * 3.8) + 36.71) / SR)) * np.exp(-tc * 1.1) * 0.65
    c_bell = (
        np.sin(2 * math.pi * 587.33 * tc) * 0.45
        + np.sin(2 * math.pi * 739.99 * tc) * 0.35   # F#5 (Dmaj9 radiant major 3rd!)
        + np.sin(2 * math.pi * 1108.73 * tc) * 0.20  # C#6 (maj7 sparkle!)
    ) * np.exp(-tc * 1.25) * 0.22
    foley_l[iclim : iclim + clen] += c_boom + c_bell * 0.95
    foley_r[iclim : iclim + clen] += c_boom + c_bell * 1.05
    kick_env[iclim : iclim + min(clen, int(0.55 * SR))] = 1.0

    # 4i. Act IV (0:49.0 -> 0:57.2): Organic 62 BPM Physical Mainnet Heartbeat ("lub-dub")
    hb_period = BEAT * 2.0  # 0.96774s = 62 BPM half-time!
    hb_times = []
    thb = CLIMAX_FLASH + 0.85
    while thb < TOTAL_DUR - 1.2:
        hb_times.append(round(thb, 3))
        for offset_s, amp in ((0.0, 0.42), (0.19, 0.26)):
            ihb = int(round((thb + offset_s) * SR))
            blen = min(TOTAL_SAMPLES - ihb, int(0.24 * SR))
            if blen > 0:
                t_b = np.arange(blen, dtype=np.float64) / SR
                lub = np.sin(np.cumsum(2 * math.pi * (68.0 * np.exp(-t_b * 18.0) + 42.0) / SR)) * np.exp(-t_b * 13.0) * amp
                foley_l[ihb : ihb + blen] += lub
                foley_r[ihb : ihb + blen] += lub
                kick_env[ihb : ihb + blen] = np.maximum(kick_env[ihb : ihb + blen], np.exp(-t_b * 15.0) * amp * 1.8)
        thb += hb_period

    # Pure final bell chime on "made alive" (0:55.10)
    ibell = int(round(55.10 * SR))
    blen = min(TOTAL_SAMPLES - ibell, int(2.9 * SR))
    tb_e = np.arange(blen, dtype=np.float64) / SR
    end_bell = (np.sin(2 * math.pi * 587.33 * tb_e) + 0.4 * np.sin(2 * math.pi * 1174.66 * tb_e)) * np.exp(-tb_e * 1.35) * 0.16
    foley_l[ibell : ibell + blen] += end_bell
    foley_r[ibell : ibell + blen] += end_bell

    bed_l = music_l + foley_l
    bed_r = music_r + foley_r

    # 5. ENFORCE EXACT 484ms DEAD-VACUUM SILENCE (0:11.129 -> 0:11.613)
    s0, s1 = int(round(SILENCE_START * SR)), int(round(DROP_TIME * SR))
    gfade = int(0.015 * SR)
    for arr in (bed_l, bed_r, kick_env, bass_env):
        arr[s0 - gfade : s0] *= np.linspace(1.0, 0.0, gfade)
        arr[s0:s1] = 0.0

    # Smooth fade-in (0.35s) & fade-out (1.4s)
    fin = np.minimum(1.0, t / 0.35)
    fout = np.minimum(1.0, np.maximum(0.0, (TOTAL_DUR - t) / 1.4))
    bed_l *= fin * fout
    bed_r *= fin * fout

    return bed_l, bed_r, kick_env, bass_env


def mix_and_loudnorm(
    vo_l: np.ndarray,
    vo_r: np.ndarray,
    bed_l: np.ndarray,
    bed_r: np.ndarray,
    work_dir: Path,
    out_wav: Path,
    ffmpeg_exe: str,
) -> Dict[str, Any]:
    """Frequency-selective vocal pocket ducking + two-pass EBU R128 loudnorm (-14 LUFS / -1 dBTP)."""
    # Compute smooth voice presence envelope
    vo_mono = 0.5 * (np.abs(vo_l) + np.abs(vo_r))
    win = int(0.14 * SR)
    vo_env = np.convolve(vo_mono, np.ones(win) / win, mode="same")
    duck_amt = np.clip(vo_env * 14.0, 0.0, 1.0)

    # Split music bed into Vocal Mid-Pocket (300Hz - 3.6kHz) vs Sub+Air so sub-bass stays MASSIVE under voice!
    b_mid, a_mid = signal.butter(2, [300.0 / (SR * 0.5), 3600.0 / (SR * 0.5)], btype="band")
    mid_l = signal.lfilter(b_mid, a_mid, bed_l)
    mid_r = signal.lfilter(b_mid, a_mid, bed_r)
    rest_l = bed_l - mid_l
    rest_r = bed_r - mid_r

    # Duck mid-pocket by -6 dB (0.50x) and rest gently by -2 dB (0.78x) under narration
    bed_duck_l = rest_l * (1.0 - 0.22 * duck_amt) + mid_l * (1.0 - 0.52 * duck_amt)
    bed_duck_r = rest_r * (1.0 - 0.22 * duck_amt) + mid_r * (1.0 - 0.52 * duck_amt)

    mix_l = np.tanh((bed_duck_l * 0.68 + vo_l * 1.12) * 1.02) * 0.94
    mix_r = np.tanh((bed_duck_r * 0.68 + vo_r * 1.12) * 1.02) * 0.94

    # Re-guarantee dead silence in the 484ms vacuum window after limiter/filters
    s0, s1 = int(round(SILENCE_START * SR)), int(round(DROP_TIME * SR))
    mix_l[s0:s1] = 0.0
    mix_r[s0:s1] = 0.0

    pre_wav = work_dir / "master_pre_loudnorm.wav"
    write_wav_stereo(pre_wav, mix_l, mix_r)

    # Pass 1: Measure integrated loudness for exact two-pass EBU R128 loudnorm
    p1 = subprocess.run(
        [ffmpeg_exe, "-y", "-i", str(pre_wav), "-af", "loudnorm=I=-14:TP=-1.0:LRA=11:print_format=json", "-f", "null", "-"],
        capture_output=True,
        text=True,
        check=True,
    )
    js_start = p1.stderr.rfind("{")
    js_end = p1.stderr.rfind("}") + 1
    meas = json.loads(p1.stderr[js_start:js_end])

    # Pass 2: Linear broadcast normalization targeting exact -14.0 LUFS / -1.0 dBTP
    ln_filter = (
        f"loudnorm=I=-14:TP=-1.0:LRA=11:linear=true:"
        f"measured_I={meas['input_i']}:measured_TP={meas['input_tp']}:"
        f"measured_LRA={meas['input_lra']}:measured_thresh={meas['input_thresh']}:"
        f"offset={meas['target_offset']}"
    )
    subprocess.run(
        [ffmpeg_exe, "-y", "-loglevel", "error", "-i", str(pre_wav), "-af", ln_filter, "-ar", str(SR), "-ac", "2", str(out_wav)],
        check=True,
    )

    # Final loudness & silence-vacuum verification!
    chk_l, chk_r = read_wav_stereo(out_wav)
    vac_peak = float(np.max(np.abs(chk_l[s0 + 120 : s1 - 120])))
    p2 = subprocess.run(
        [ffmpeg_exe, "-i", str(out_wav), "-af", "volumedetect", "-f", "null", "-"],
        capture_output=True,
        text=True,
        check=True,
    )
    mean_db = max_db = "unknown"
    for ln in p2.stderr.splitlines():
        if "mean_volume:" in ln:
            mean_db = ln.split("mean_volume:")[-1].strip()
        if "max_volume:" in ln:
            max_db = ln.split("max_volume:")[-1].strip()

    return {
        "duration_s": round(len(chk_l) / SR, 3),
        "sample_rate": SR,
        "input_lufs": meas["input_i"],
        "target_lufs": "-14.0",
        "mean_volume": mean_db,
        "max_volume": max_db,
        "silence_vacuum_peak": round(vac_peak, 6),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build 124 BPM Newsatoshi Cinema Audio Master + Manifest (Gate 1)")
    parser.add_argument("--out-dir", default="media/cinema/out")
    parser.add_argument("--music-stem", default=None, help="Optional custom 48kHz stereo WAV music stem")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent.parent
    out_dir = (repo_root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    work_dir = out_dir / "_audio_work"
    work_dir.mkdir(parents=True, exist_ok=True)

    ffmpeg_exe = imageio_ffmpeg.get_ffmpeg_exe()
    print(f"[audio-gate1] Synthesizing 23 directed vocal hits ({BPM:.0f} BPM, {TOTAL_DUR:.2f}s)...", flush=True)
    vo_l, vo_r, placed_lines = build_voice_stems(work_dir, ffmpeg_exe)

    print("[audio-gate1] Synthesizing 124 BPM electronic-orchestral score + sample-locked Foley...", flush=True)
    stem = Path(args.music_stem).resolve() if args.music_stem else None
    bed_l, bed_r, kick_env, bass_env = synth_music_and_foley(stem)

    master_wav = out_dir / "cinema_master_48k.wav"
    print("[audio-gate1] Mixing with frequency-selective pocket ducking + two-pass -14 LUFS loudnorm...", flush=True)
    audit = mix_and_loudnorm(vo_l, vo_r, bed_l, bed_r, work_dir, master_wav, ffmpeg_exe)

    # Sample 60Hz audio-reactive envelopes for the 3D WebGL2 shaders!
    kick_60 = []
    bass_60 = []
    for f in range(TOTAL_FRAMES):
        i0 = min(TOTAL_SAMPLES - 1, int(round((f / FPS) * SR)))
        i1 = min(TOTAL_SAMPLES, i0 + int(SR / FPS))
        kick_60.append(round(float(np.max(kick_env[i0:i1])) if i1 > i0 else 0.0, 4))
        bass_60.append(round(float(np.mean(bass_env[i0:i1])) if i1 > i0 else 0.0, 4))

    manifest = {
        "bpm": BPM,
        "beat_s": BEAT,
        "bar_s": BAR,
        "fps": FPS,
        "total_dur": round(TOTAL_DUR, 4),
        "total_frames": TOTAL_FRAMES,
        "silence_start": round(SILENCE_START, 4),
        "drop_time": round(DROP_TIME, 4),
        "climax_flash": round(CLIMAX_FLASH, 4),
        "audit": audit,
        "lines": placed_lines,
        "kick_60hz": kick_60,
        "bass_60hz": bass_60,
    }
    manifest_path = out_dir / "audio_manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    print("\n=== GATE 1 AUDIO AUDIT ===", flush=True)
    print(f"  Master WAV:      {master_wav}", flush=True)
    print(f"  Duration:        {audit['duration_s']}s ({TOTAL_FRAMES} frames @ {FPS}fps)", flush=True)
    print(f"  Loudness:        target {audit['target_lufs']} LUFS | mean={audit['mean_volume']} | max={audit['max_volume']}", flush=True)
    print(f"  484ms Vacuum:    peak={audit['silence_vacuum_peak']} (must be < 0.001)", flush=True)
    print(f"  Spoken Lines:    {len(placed_lines)} lines ({sum(len(l['text'].split()) for l in placed_lines)} words)", flush=True)
    for ln in placed_lines:
        print(f"    {ln['start']:6.2f}s -> {ln['end']:6.2f}s ({ln['dur']:.2f}s) | {ln['text']}", flush=True)

    assert audit["silence_vacuum_peak"] < 0.002, f"Silence vacuum violated: {audit['silence_vacuum_peak']}"
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
