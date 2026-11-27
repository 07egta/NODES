#!/usr/bin/env python3
"""Generate independent sum/mean answers using exact Python rational arithmetic.

Inputs and expected results are binary64 bit strings. Fraction.from_float reads
the exact represented input; all arithmetic stays rational until one final
float conversion (round to nearest, ties to even). No NODES code is imported.
Run from any directory; --check verifies the checked-in reproducible fixture.
"""

from __future__ import annotations

import argparse
from fractions import Fraction
import json
from pathlib import Path
import struct

MASK = (1 << 64) - 1
FRACTION = (1 << 52) - 1
DESTINATION = Path(__file__).resolve().parents[1] / "tests/fixtures/sum_oracle.json"


def from_bits(bits: int) -> float:
    return struct.unpack(">d", struct.pack(">Q", bits))[0]


def rounded_bits(value: Fraction) -> str | None:
    try:
        rounded = float(value)
    except OverflowError:
        return None
    return f"{struct.unpack('>Q', struct.pack('>d', rounded))[0]:016x}"


def generate() -> str:
    state = 0x5A17C0DE20261127

    def random_word() -> int:
        nonlocal state
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        return state

    def finite_word() -> int:
        bits = random_word()
        if (bits >> 52) & 0x7FF == 0x7FF:
            bits ^= 1 << 52
        return bits

    cases = []

    def add(name: str, bits: list[int]) -> None:
        values = [Fraction.from_float(from_bits(word)) for word in bits]
        total = sum(values, Fraction())
        cases.append({
            "id": name,
            "input_bits": [f"{word:016x}" for word in bits],
            "sum_bits": rounded_bits(total),
            "mean_bits": rounded_bits(total / len(values)),
        })

    for index in range(48):
        add(f"wide-{index:02}", [finite_word() for _ in range(2 + random_word() % 31)])

    exponents = [0, 1, 2, 63, 64, 65, 511, 1022, 1023, 1024, 1534, 2045, 2046]
    for index in range(32):
        bits = []
        for _ in range(3 + random_word() % 27):
            word = random_word()
            exponent = exponents[random_word() % len(exponents)]
            bits.append((word & ((1 << 63) | FRACTION)) | (exponent << 52))
        add(f"exponent-boundaries-{index:02}", bits)

    for index in range(32):
        large = finite_word() & ~(1 << 63)
        residual = finite_word()
        bits = [large, residual, large | (1 << 63)]
        for _ in range(1 + random_word() % 9):
            paired = finite_word()
            bits.extend([paired, paired ^ (1 << 63)])
        add(f"cancellation-{index:02}", bits)

    for index in range(16):
        # Ordinary values around 1, including adjacent floats and both signs.
        bits = [
            (random_word() & ((1 << 63) | ((1 << 18) - 1))) | (1023 << 52)
            for _ in range(5 + index)
        ]
        add(f"near-one-{index:02}", bits)

    assert len(cases) == 128
    metadata = {
        "schema_version": 1,
        "oracle": "Python fractions.Fraction exact input sum; one final binary64 round-to-nearest/even",
        "generator": "scripts/generate_sum_oracle.py",
        "seed_hex": "5a17c0de20261127",
        "overflow": "null means correctly rounded binary64 sum overflows",
        "zero": "Exact cancellation is +0; a negative nonzero quotient may round to -0",
    }
    # One case per line keeps input bit patterns easy to review without a large fixture.
    prefix = json.dumps(metadata, ensure_ascii=False, indent=2)[:-2]
    return prefix + ',\n  "cases": [\n' + ",\n".join(
        "    " + json.dumps(case, separators=(",", ":")) for case in cases
    ) + "\n  ]\n}\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    generated = generate()
    # Parse the serialized form before writing, independent of its compact layout.
    assert len(json.loads(generated)["cases"]) == 128
    if args.check:
        if DESTINATION.read_text(encoding="utf-8") != generated:
            raise SystemExit("sum oracle fixture differs; run scripts/generate_sum_oracle.py")
        print("sum oracle: 128 reproducible exact-rational cases verified")
    else:
        DESTINATION.parent.mkdir(parents=True, exist_ok=True)
        DESTINATION.write_text(generated, encoding="utf-8")
        print(f"wrote {len(generated)} bytes to {DESTINATION}")


if __name__ == "__main__":
    main()
