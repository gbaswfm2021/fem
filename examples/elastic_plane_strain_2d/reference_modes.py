"""Independent free-rectangle in-plane Rayleigh-Ritz reference (requires NumPy).

Legendre polynomials approximate the displacement, not the FEM mesh.  The
free-boundary variational problem includes three rigid motions.  This is a
convergent 2D reference, not a full 3D thin-body calculation.
"""
import argparse
import json
import numpy as np
from numpy.polynomial import legendre


def reference(formulation, degree):
    length, width, young, nu, rho = 0.2, 0.1, 70e9, 0.33, 2700.0
    mu = young / (2 * (1 + nu))
    lam = young * nu / ((1 + nu) * (1 - 2 * nu))
    if formulation == "planeStress":
        lam = 2 * lam * mu / (lam + 2 * mu)
    points, weights = legendre.leggauss(max(16, 2 * degree + 3))
    x, y = np.meshgrid(points, points, indexing="ij")
    wx, wy = np.meshgrid(weights, weights, indexing="ij")
    w = (wx * wy * length * width / 4).ravel()
    cols = []
    for i in range(degree + 1):
        for j in range(degree + 1):
            a, b = legendre.Legendre.basis(i), legendre.Legendre.basis(j)
            cols.append(((a(x) * b(y)).ravel(), (a.deriv()(x) * b(y) * 2 / length).ravel(),
                         (a(x) * b.deriv()(y) * 2 / width).ravel()))
    val, dx, dy = (np.column_stack(v) for v in zip(*cols))
    gram = lambda a, b: a.T @ (w[:, None] * b)
    mass0 = rho * gram(val, val)
    kxx = (lam + 2 * mu) * gram(dx, dx) + mu * gram(dy, dy)
    kyy = (lam + 2 * mu) * gram(dy, dy) + mu * gram(dx, dx)
    kxy = lam * gram(dx, dy) + mu * gram(dy, dx)
    k = np.block([[kxx, kxy], [kxy.T, kyy]])
    mass = np.block([[mass0, np.zeros_like(mass0)], [np.zeros_like(mass0), mass0]])
    chol = np.linalg.cholesky(mass)
    tmp = np.linalg.solve(chol, k)
    op = np.linalg.solve(chol, tmp.T).T
    eigenvalues, eigenvectors = np.linalg.eigh((op + op.T) / 2)
    vectors = np.linalg.solve(chol.T, eigenvectors)
    count = (degree + 1) ** 2
    edge = np.array([(-1.) ** i * (width if j == 0 else 0.)
                     for i in range(degree + 1) for j in range(degree + 1)])
    return [(np.sqrt(v) / (2 * np.pi), abs(edge @ vectors[count:, idx]), vectors[:, idx])
            for idx, v in enumerate(eigenvalues) if v > 1.0]


def shape_correlation(path, coefficients, degree):
    with open(path, encoding="utf-8") as handle:
        field = json.load(handle)
    nodes = np.asarray(field["nodes"])
    x, y = nodes[:, 0], nodes[:, 1]
    basis = np.column_stack([legendre.Legendre.basis(i)(2*x/0.2-1)*legendre.Legendre.basis(j)(2*y/0.1-1)
                             for i in range(degree + 1) for j in range(degree + 1)])
    n = (degree + 1) ** 2
    target = np.r_[basis @ coefficients[:n], basis @ coefficients[n:]]
    components = field["fields"]
    solved = np.r_[np.asarray(components["ux"]["real"]) + 1j*np.asarray(components["ux"]["imag"]),
                   np.asarray(components["uy"]["real"]) + 1j*np.asarray(components["uy"]["imag"])]
    zero, one = np.zeros_like(x), np.ones_like(x)
    rigid = np.column_stack((np.r_[one,zero],np.r_[zero,one],np.r_[-y,x]))
    target -= rigid @ np.linalg.lstsq(rigid,target,rcond=None)[0]
    solved -= rigid @ np.linalg.lstsq(rigid,solved,rcond=None)[0]
    return abs(np.vdot(target,solved)) / (np.linalg.norm(target)*np.linalg.norm(solved))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--formulation", choices=["planeStress", "planeStrain"], default="planeStress")
    parser.add_argument("--degree", type=int, default=8)
    parser.add_argument("--limit", type=int, default=8)
    parser.add_argument("--compare-field")
    parser.add_argument("--mode", type=int, default=0, help="zero-based nonrigid reference mode")
    args = parser.parse_args()
    modes = reference(args.formulation, args.degree)
    for frequency, coupling, _ in modes[:args.limit]:
        print(f"{frequency:.4f} Hz  edge coupling={coupling:.6e}")
    if args.compare_field:
        print(f"mode shape correlation: {shape_correlation(args.compare_field, modes[args.mode][2], args.degree):.6f}")
