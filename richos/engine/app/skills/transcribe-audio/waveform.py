"""Print 96 single-digit peak levels from a mono, 16-bit PCM WAV."""

import sys
import wave
from array import array


def peak_levels(path):
    with wave.open(path, "rb") as audio:
        if audio.getnchannels() != 1 or audio.getsampwidth() != 2:
            raise ValueError("Expected a mono, 16-bit PCM WAV.")
        frames = audio.getnframes()
        previous = 0
        peaks = []
        for index in range(1, 97):
            end = frames * index // 96
            remaining = end - previous
            peak = 0
            while remaining:
                block = array("h")
                block.frombytes(audio.readframes(min(remaining, 16384)))
                if not block:
                    raise ValueError("The WAV ended before its declared length.")
                if sys.byteorder != "little":
                    block.byteswap()
                peak = max(peak, max(block), -min(block))
                remaining -= len(block)
            peaks.append(peak)
            previous = end
    scale = max(max(peaks), 1)
    return "".join(str(round(9 * peak / scale)) for peak in peaks)


if __name__ == "__main__":
    print(peak_levels(sys.argv[1]))
