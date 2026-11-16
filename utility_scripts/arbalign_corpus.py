#!/usr/bin/env python3
"""Generate the deterministic synthetic arbalign corpora.

Both corpora use the same composition, four water and two ammonia molecules:
O4N2H14 (20 atoms). Structures differ by family-level ring geometry and local
orientation, with small within-family perturbations so deduplication has real
work to do. The generator is deterministic and needs no third-party packages.

Usage:
    utility_scripts/arbalign_corpus.py --out tests/arbalign_fixtures/corpus \
        --families 4 --variants 4
"""

import argparse
import math
import pathlib


class Lcg:
    def __init__(self, seed):
        self.state = seed & 0xFFFFFFFF

    def next(self):
        self.state = (1664525 * self.state + 1013904223) & 0xFFFFFFFF
        return self.state / 0x100000000

    def signed(self):
        return 2.0 * self.next() - 1.0


def rotate(point, ax, ay, az):
    x, y, z = point
    c, s = math.cos(az), math.sin(az)
    x, y = x * c - y * s, x * s + y * c
    c, s = math.cos(ay), math.sin(ay)
    x, z = x * c + z * s, -x * s + z * c
    c, s = math.cos(ax), math.sin(ax)
    y, z = y * c - z * s, y * s + z * c
    return x, y, z


def translate(points, shift):
    return [tuple(a + b for a, b in zip(point, shift)) for point in points]


def water(rng, jitter):
    scale = 1.0 + 0.015 * rng.signed() * jitter
    return [
        ("O", (0.0, 0.0, 0.0)),
        ("H", (0.96 * scale, 0.0, 0.0)),
        ("H", (-0.24 * scale, 0.93 * scale, 0.0)),
    ]


def ammonia(rng, jitter):
    scale = 1.0 + 0.015 * rng.signed() * jitter
    direction = 1.0 / math.sqrt(3.0)
    return [
        ("N", (0.0, 0.0, 0.0)),
        ("H", (1.01 * scale * direction, 1.01 * scale * direction, 1.01 * scale * direction)),
        ("H", (1.01 * scale * direction, -1.01 * scale * direction, -1.01 * scale * direction)),
        ("H", (-1.01 * scale * direction, 1.01 * scale * direction, -1.01 * scale * direction)),
    ]


def molecule_points(rng, family, molecule, jitter):
    if molecule < 4:
        points = water(rng, jitter)
    else:
        points = ammonia(rng, jitter)
    base = (
        0.21 * family,
        0.17 * family + 0.4 * molecule,
        0.29 * family - 0.3 * molecule,
    )
    return [
        (symbol, rotate(point, *(base[i] + 0.05 * jitter * rng.signed() for i in range(3))))
        for symbol, point in points
    ]


def cluster(family, variant, jitter):
    rng = Lcg(0xA11A0000 + family * 1009 + variant * 9176)
    radius = 3.0 + 0.17 * family
    atoms = []
    for molecule in range(6):
        angle = 2.0 * math.pi * molecule / 6.0 + 0.05 * family
        center = (
            radius * math.cos(angle) + 0.03 * jitter * rng.signed(),
            radius * math.sin(angle) + 0.03 * jitter * rng.signed(),
            0.35 * math.sin(2.0 * angle + 0.3 * family)
            + 0.03 * jitter * rng.signed(),
        )
        local = molecule_points(rng, family, molecule, jitter)
        atoms.extend((symbol, tuple(a + b for a, b in zip(point, center))) for symbol, point in local)
    rotation = (0.4 * rng.signed(), 0.4 * rng.signed(), 0.4 * rng.signed())
    shift = (4.0 * rng.signed(), 4.0 * rng.signed(), 4.0 * rng.signed())
    return [(symbol, rotate(point, *rotation)) for symbol, point in atoms], shift


def write_corpus(directory, families, variants, jitter, prefix):
    directory = pathlib.Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    for old in directory.glob("*.xyz"):
        old.unlink()
    count = 0
    for family in range(families):
        for variant in range(variants):
            atoms, shift = cluster(family, variant, jitter)
            path = directory / f"{prefix}{count:03d}.xyz"
            with path.open("w") as handle:
                handle.write(f"{len(atoms)}\n{prefix} family {family} variant {variant}\n")
                for symbol, point in atoms:
                    point = tuple(a + b for a, b in zip(point, shift))
                    handle.write(
                        f"{symbol} {point[0]:.10f} {point[1]:.10f} {point[2]:.10f}\n"
                    )
            count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("--families", type=int, default=4)
    parser.add_argument("--variants", type=int, default=4)
    parser.add_argument("--jitter", type=float, default=1.0)
    parser.add_argument("--prefix", default="m")
    args = parser.parse_args()
    count = write_corpus(args.out, args.families, args.variants, args.jitter, args.prefix)
    print(f"wrote {count} structures into {args.out}")


if __name__ == "__main__":
    main()
