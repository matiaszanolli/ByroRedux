// Normal covectors follow the inverse transpose. A weighted blend of bone
// rotations need not remain orthogonal even when each bone has uniform scale.
mat3 surfaceNormalTransform(mat3 transform, bool needsInverseTranspose) {
    if (needsInverseTranspose && abs(determinant(transform)) > 1e-6) {
        return transpose(inverse(transform));
    }
    return transform;
}
