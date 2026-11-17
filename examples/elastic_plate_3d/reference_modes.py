"""Independent Kirchhoff-Love free-plate Rayleigh-Ritz frequency reference.

Requires NumPy. The 2D thin-plate model differs from the 3D tetrahedral
solver; this script is a comparison, not an exact 3D reference. The
tensor-product Legendre basis includes rigid translation and rotations.
"""
import argparse
import json
import numpy as np
from numpy.polynomial import legendre


def modes(degree, length=0.2, width=0.1, thickness=0.004,
          young=70e9, poisson=0.33, density=2700.0):
    points, weights = legendre.leggauss(max(2 * degree + 3, 16))
    x, y = np.meshgrid(points, points, indexing="ij")
    wx, wy = np.meshgrid(weights, weights, indexing="ij")
    measure = (wx * wy * length * width / 4).ravel()
    columns = []
    for i in range(degree + 1):
        xi = legendre.Legendre.basis(i)
        for j in range(degree + 1):
            eta = legendre.Legendre.basis(j)
            value = (xi(x) * eta(y)).ravel()
            xx = (xi.deriv(2)(x) * eta(y) * (2 / length) ** 2).ravel()
            yy = (xi(x) * eta.deriv(2)(y) * (2 / width) ** 2).ravel()
            xy = (xi.deriv()(x) * eta.deriv()(y) * 4 / (length * width)).ravel()
            columns.append((value, xx, yy, xy))
    value, xx, yy, xy = [np.column_stack(v) for v in zip(*columns)]
    def gram(left, right):
        return left.T @ (measure[:, None] * right)
    bending = young * thickness ** 3 / (12 * (1 - poisson ** 2))
    stiffness = bending * (gram(xx, xx) + poisson * (gram(xx, yy) + gram(yy, xx))
                           + gram(yy, yy) + 2 * (1 - poisson) * gram(xy, xy))
    mass = density * thickness * gram(value, value)
    chol = np.linalg.cholesky(mass)
    tmp = np.linalg.solve(chol, stiffness)
    operator = np.linalg.solve(chol, tmp.T).T
    eigenvalues, vectors = np.linalg.eigh((operator + operator.T) / 2)
    modes_ = np.linalg.solve(chol.T, vectors)
    # Uniform transverse traction across the full x=0 side: line integral.
    edge = np.array([(-1.0) ** i * (width if j == 0 else 0.0)
                     for i in range(degree + 1) for j in range(degree + 1)])
    coupling = np.abs(edge @ modes_)
    return [(np.sqrt(max(v, 0)) / (2 * np.pi), coupling[i], modes_[:, i])
            for i, v in enumerate(eigenvalues) if v > 1e-4]


def compare_field(path, degree, coefficients):
    """Compare transverse top-surface shapes after removing rigid motion."""
    with open(path, encoding="utf-8") as handle:
        field = json.load(handle)
    nodes = np.asarray(field["nodes"])
    selected = np.isclose(nodes[:, 2], 0.004, atol=1e-9)
    x, y = nodes[selected, 0], nodes[selected, 1]
    values = field["fields"]["uz"]
    displacement = (np.asarray(values["real"]) + 1j * np.asarray(values["imag"]))[selected]
    basis = np.column_stack([
        legendre.Legendre.basis(i)(2 * x / 0.2 - 1)
        * legendre.Legendre.basis(j)(2 * y / 0.1 - 1)
        for i in range(degree + 1) for j in range(degree + 1)
    ])
    reference = basis @ coefficients
    rigid = np.column_stack((np.ones_like(x), x, y))
    reference -= rigid @ np.linalg.lstsq(rigid, reference, rcond=None)[0]
    displacement -= rigid @ np.linalg.lstsq(rigid, displacement, rcond=None)[0]
    return abs(np.vdot(reference, displacement)) / (
        np.linalg.norm(reference) * np.linalg.norm(displacement)), len(x)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--degree", type=int, default=10)
    parser.add_argument("--limit", type=int, default=15)
    parser.add_argument("--compare-field", help="elastic field JSON to compare to the first bending mode")
    args = parser.parse_args()
    reference_modes = modes(args.degree)
    for frequency, coupling, _ in reference_modes[:args.limit]:
        print(f"{frequency:.6f} Hz  uniform-edge coupling={coupling:.6e}")
    if args.compare_field:
        correlation, count = compare_field(args.compare_field, args.degree, reference_modes[0][2])
        print(f"first bending mode vs field: correlation={correlation:.6f}  top nodes={count}")
