"""The sample sounds, made from scratch (no recordings): short WAV loops and one one-shot,
mono 16-bit at 16 kHz, each a few dozen kilobytes.

    python tools/make_sounds.py [content/assets/sounds]

wind.wav      a loop of soft, slowly swelling noise (an ambience)
crickets.wav  a loop of chirps (a night ambience)
hum.wav       a low hum (a prop's sound: the stone well)
chime.wav     a short chime (a script's one-shot)
"""
import math
import random
import struct
import sys
import wave
from pathlib import Path

RATE = 16000


def write(path, samples):
    path.parent.mkdir(parents=True, exist_ok=True)
    peak = max(1e-6, max(abs(s) for s in samples))
    k = 0.8 / peak if peak > 0.8 else 1.0
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s * k)) * 32767)) for s in samples))
    print(f"{path} ({path.stat().st_size // 1024} KB)")


def seamless(samples, fade):
    """The end blended into the start, so the loop has no seam."""
    n = len(samples)
    out = list(samples)
    for i in range(fade):
        t = i / fade
        out[i] = samples[i] * t + samples[n - fade + i] * (1 - t)
    return out[: n - fade]


def wind(seconds=3.0):
    rng = random.Random(7)
    n = int(RATE * seconds)
    raw = [rng.uniform(-1, 1) for _ in range(n)]
    # smoothed twice: a soft rush; swelling slowly
    out = []
    a = b = 0.0
    for i, r in enumerate(raw):
        a += (r - a) * 0.02
        b += (a - b) * 0.02
        swell = 0.55 + 0.45 * math.sin(2 * math.pi * i / (RATE * 1.5)) * math.sin(2 * math.pi * i / (RATE * 0.7) + 1.0)
        out.append(b * 6.0 * swell)
    return seamless(out, RATE // 4)


def crickets(seconds=2.0):
    n = int(RATE * seconds)
    out = [0.0] * n
    for start in (0.0, 0.1, 0.2, 0.9, 1.0, 1.1, 1.6):
        s0 = int(start * RATE)
        for i in range(int(0.07 * RATE)):
            if s0 + i >= n:
                break
            t = i / RATE
            env = math.sin(math.pi * i / (0.07 * RATE))
            trill = 0.5 + 0.5 * math.sin(2 * math.pi * 38 * t)
            out[s0 + i] += env * trill * math.sin(2 * math.pi * 4300 * t) * 0.6
    return seamless(out, RATE // 20)


def hum(seconds=1.0):
    n = int(RATE * seconds)
    return [0.5 * math.sin(2 * math.pi * 110 * i / RATE) + 0.25 * math.sin(2 * math.pi * 220 * i / RATE + 0.5) + 0.1 * math.sin(2 * math.pi * 330 * i / RATE) for i in range(n)]


def chime(seconds=0.7):
    n = int(RATE * seconds)
    out = []
    for i in range(n):
        t = i / RATE
        env = math.exp(-5.0 * t)
        out.append(env * (0.6 * math.sin(2 * math.pi * 880 * t) + 0.3 * math.sin(2 * math.pi * 1320 * t) + 0.15 * math.sin(2 * math.pi * 1760 * t)))
    return out


if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "content/assets/sounds")
    write(out / "wind.wav", wind())
    write(out / "crickets.wav", crickets())
    write(out / "hum.wav", hum())
    write(out / "chime.wav", chime())
