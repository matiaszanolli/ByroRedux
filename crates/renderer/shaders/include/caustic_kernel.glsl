#ifndef BYRO_CAUSTIC_KERNEL_GLSL
#define BYRO_CAUSTIC_KERNEL_GLSL

// Normalised 5x5 Gaussian footprint (sigma = 1). Both caustic writers are
// composited after TAA, so this shared spatial filter is their only per-frame
// smoothing. Keeping one table also pins the two paths to the same energy
// distribution without rebuilding the kernel for every source pixel.
const float CAUSTIC_GAUSS5[25] = float[25](
    0.0029690167, 0.0133062099, 0.0219382313, 0.0133062099, 0.0029690167,
    0.0133062099, 0.0596342954, 0.0983203313, 0.0596342954, 0.0133062099,
    0.0219382313, 0.0983203313, 0.1621028216, 0.0983203313, 0.0219382313,
    0.0133062099, 0.0596342954, 0.0983203313, 0.0596342954, 0.0133062099,
    0.0029690167, 0.0133062099, 0.0219382313, 0.0133062099, 0.0029690167
);

float causticGauss5Weight(int x, int y) {
    return CAUSTIC_GAUSS5[(y + 2) * 5 + (x + 2)];
}

#endif
