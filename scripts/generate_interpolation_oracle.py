#!/usr/bin/env python3
"""Generate an independent, reproducible exact-rational interpolation oracle.

All inputs are finite binary64 bit patterns. Fraction.from_float recovers their
exact values; a + t*b - t*a stays rational until one final float conversion.
No NODES implementation, compensated arithmetic, or fixed-width accumulator is
used. --check verifies the checked-in fixture without changing any files.
"""

from __future__ import annotations

import argparse
from fractions import Fraction
import json
import math
from pathlib import Path
import struct

MASK = (1 << 64) - 1
SIGN = 1 << 63
MAX_BITS = 0x7FEFFFFFFFFFFFFF
SEED = 0x4C45525020261126
DESTINATION = Path(__file__).resolve().parents[1] / "tests/fixtures/interpolation_oracle.json"


def from_bits(word: int) -> float:
    return struct.unpack(">d", struct.pack(">Q", word))[0]


def to_bits(value: float) -> int:
    return struct.unpack(">Q", struct.pack(">d", value))[0]


def expected_bits(a: float, b: float, t: float) -> str | None:
    # These are explicit API contracts, including signed-zero endpoint bits.
    if t == 0.0:
        return f"{to_bits(a):016x}"
    if t == 1.0:
        return f"{to_bits(b):016x}"
    if to_bits(a) == to_bits(b):
        return f"{to_bits(a):016x}"
    aa, bb, tt = map(Fraction.from_float, (a, b, t))
    exact = aa + tt * bb - tt * aa
    try:
        result = float(exact)
    except OverflowError:
        return None
    # Fraction(0) converts to +0; negative nonzero underflow converts to -0.
    return f"{to_bits(result):016x}"


def generate() -> str:
    cases: list[dict] = []
    state = SEED

    def random_word() -> int:
        nonlocal state
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        return state

    def random_finite() -> float:
        word = random_word()
        if (word >> 52) & 0x7FF == 0x7FF:
            word ^= 1 << 52
        return from_bits(word)

    def add(name: str, group: str, a: float, b: float, t: float,
            note: str = "", previous_candidate: dict | None = None) -> None:
        assert all(map(math.isfinite, (a, b, t)))
        case = {
            "id": name,
            "category": group,
            "input_bits": [f"{to_bits(x):016x}" for x in (a, b, t)],
            "input_hex": [x.hex() for x in (a, b, t)],
            "expected_bits": expected_bits(a, b, t),
        }
        if note:
            case["note"] = note
        if previous_candidate is not None:
            case["previous_candidate_observation"] = previous_candidate
        cases.append(case)

    maximum = from_bits(MAX_BITS)
    tiny = from_bits(1)
    below_one = math.nextafter(1.0, 0.0)
    above_one = math.nextafter(1.0, math.inf)
    below_half = math.nextafter(0.5, 0.0)

    for name, a, b, t, old in [
        ("opposite-near-one", -1.0, 2.0**-53, below_one, (-2.0**-53).hex()),
        ("positive-near-one", 1.0, 2.0**-54, below_one, (2.0**-53).hex()),
        ("extrapolate-near-one", 2.0**54, 1.0, above_one, (-4.0).hex()),
    ]:
        add(name, "rounded_difference_loss", a, b, t,
            "A rounded b-a loses a term that matters after cancellation.",
            {"algorithm": "fma(t, rounded(b-a), a)", "observed_hex": old})

    for sign in [-1.0, 1.0]:
        for index, t in enumerate([below_half, 0.5, math.nextafter(0.5, 1.0)]):
            a = sign * 2.0**1023
            b = -a
            observation = None
            if t == below_half:
                observation = {
                    "algorithm": "rounded weighted endpoints (1-t)*a+t*b",
                    "observed_hex": (sign * 2.0**969).hex(),
                }
            add(f"overflowing-difference-{sign:+.0f}-{index}",
                "opposite_extrema_interpolation", a, b, t,
                "The exact difference exceeds binary64; each endpoint is finite.", observation)

    for index, (a, b, t) in enumerate([
        (1e308, 1.0000000000000002e308, 2.0),
        (1e308, 7e307, 5.0),
        (maximum, -maximum, maximum),
        (-maximum, maximum, -maximum),
    ]):
        add(f"large-extrapolation-{index}", "finite_and_overflow_extrapolation", a, b, t)

    for sign in [-1.0, 1.0]:
        for offset in [0, 1, 2, 3]:
            t = sign * from_bits(MAX_BITS - offset)
            observation = None
            if offset == 1:
                observation = {
                    "algorithm": "FastTwoSum difference followed by two FMAs",
                    "observed_hex": "inf" if sign < 0 else "-inf",
                    "half_endpoint_scaling_also_failed": True,
                }
            add(f"staged-overflow-{sign:+.0f}-{offset}", "overflow_rounding_boundary",
                1.0, -3.0 * 2.0**-54, t,
                "offset 0 overflows; adjacent finite-t boundary cases must round only once.",
                observation)

    for index, (a, b, t) in enumerate([
        ("-0x1.307b5b4ff9606p+1023", "-0x1.6dad2a5fe379cp+984", "-0x1.5cf37c908cecfp-1"),
        ("-0x1.2d90e233fe9e6p+1023", "0x1.9ad7e202e3934p+999", "0x1.5951a5bfab5bep+1"),
    ]):
        add(f"rounded-overflow-hidden-{index}", "overflow_rounding_boundary",
            *map(float.fromhex, (a, b, t)),
            note="Two-stage FMA returned a finite extremum although the correctly rounded result overflows.")

    for index, (a, b, t) in enumerate([
        (-0.0, maximum, 0.0), (-0.0, maximum, -0.0),
        (maximum, -0.0, 1.0), (-maximum, maximum, 0.0),
        (-maximum, maximum, 1.0), (maximum, -maximum, 1.0),
    ]):
        add(f"endpoint-{index}", "endpoint_bits", a, b, t)

    for index, a in enumerate([tiny, -tiny, -0.0, 0.0, maximum, -maximum]):
        add(f"constant-{index}", "constant_bits", a, a, maximum if index % 2 else -maximum)
    for index, (a, b) in enumerate([(-0.0, 0.0), (0.0, -0.0), (1.0, -1.0)]):
        add(f"exact-zero-{index}", "exact_cancellation_positive_zero", a, b, 0.5)

    for index, (a, b, t) in enumerate([
        (0.0, tiny, 0.5), (-tiny, 0.0, 0.5),
        (tiny, 2.0*tiny, 0.5), (2.0*tiny, 3.0*tiny, 0.5),
        (-tiny, -2.0*tiny, 0.5), (-2.0*tiny, -3.0*tiny, 0.5),
        (-tiny, tiny, 0.25), (-tiny, tiny, 0.75),
        (from_bits((1 << 52)-1), from_bits(1 << 52), 0.5),
        (-from_bits((1 << 52)-1), -from_bits(1 << 52), 0.5),
        (0.0, tiny, tiny), (0.0, -tiny, tiny),
        (tiny, 2.0*tiny, -maximum),
    ]):
        add(f"subnormal-{index:02}", "subnormal_rounding", a, b, t)

    for index in range(32):
        add(f"wide-{index:02}", "logarithmic_finite_random",
            random_finite(), random_finite(), random_finite())

    # Parameters adjacent to exact cancellation points exercise products whose
    # exponents differ greatly. These values are generated from rational input,
    # independently of the implementation's product decomposition.
    for index in range(16):
        a, b = random_finite(), random_finite()
        if a == b:
            b = -a if a != 0.0 else 1.0
        try:
            t = float(-Fraction(a) / (Fraction(b)-Fraction(a)))
        except OverflowError:
            t = maximum
        t = math.nextafter(t, -math.inf if index % 2 else math.inf)
        if not math.isfinite(t):
            t = math.copysign(maximum, t)
        add(f"near-cancellation-{index:02}", "near_exact_cancellation", a, b, t)

    # Fill to 128 with directed parameters near 0, 1/2, 1 and 2, plus tiny and
    # maximum finite extrapolations. Inputs span normal/subnormal exponent bins.
    directed_t = [tiny, -tiny, below_half, 0.5, below_one, above_one, -1.0, 2.0, maximum]
    exponent_fields = [0, 1, 2, 63, 511, 1022, 1023, 1024, 1534, 2045, 2046]
    while len(cases) < 128:
        index = len(cases)
        words = [
            (random_word() & (SIGN | ((1 << 52)-1)))
            | (exponent_fields[random_word() % len(exponent_fields)] << 52)
            for _ in range(2)
        ]
        add(f"directed-{index:03}", "directed_exponent_boundaries",
            from_bits(words[0]), from_bits(words[1]), directed_t[index % len(directed_t)])

    assert len(cases) == 128
    assert len({case["id"] for case in cases}) == 128
    metadata = {
        "schema_version": 1,
        "generator": "scripts/generate_interpolation_oracle.py",
        "seed_hex": f"{SEED:016x}",
        "oracle": "Exact fractions.Fraction a+t*b-t*a, then one binary64 round-to-nearest/even",
        "input_order": ["a", "b", "t"],
        "overflow": "null expected_bits means the correctly rounded result is infinite",
        "special_values": "Finite inputs only. t==0/1 returns endpoint bits; identical endpoint bits return that constant; other exact zero is +0; negative nonzero underflow is -0.",
        "previous_candidate_observations": "Selected independently reproduced rejected formulas, preserved for audit. They are not outputs from the final exact implementation.",
        "exploratory_review": {
            "method": "Python Fraction versus libm.fma; structured/logarithmic inputs and neighbors of +/-MAX rounding thresholds",
            "seed_decimal": 20261126,
            "unique_input_count": 48149,
            "two_fma_false_nonfinite": 19,
            "two_fma_false_finite": 6,
            "scope": "Earlier read-only exploratory sweep; this generator reproduces the 128-case fixture, not that entire sweep. No exhaustive correctness claim.",
        },
    }
    prefix = json.dumps(metadata, ensure_ascii=False, indent=2)[:-2]
    return prefix + ',\n  "cases": [\n' + ",\n".join(
        "    " + json.dumps(case, separators=(",", ":")) for case in cases
    ) + "\n  ]\n}\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    generated = generate()
    assert len(json.loads(generated)["cases"]) == 128
    if args.check:
        if not DESTINATION.exists() or DESTINATION.read_text(encoding="utf-8") != generated:
            raise SystemExit("interpolation oracle differs; run scripts/generate_interpolation_oracle.py")
        print("interpolation oracle: 128 reproducible exact-rational cases verified")
    else:
        DESTINATION.parent.mkdir(parents=True, exist_ok=True)
        DESTINATION.write_text(generated, encoding="utf-8")
        print(f"wrote {len(generated)} bytes to {DESTINATION}")


if __name__ == "__main__":
    main()
