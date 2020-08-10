uint hash_uint(uint x) {
    x += (x << 10u);
    x ^= (x >> 6u);
    x += (x << 3u);
    x ^= (x >> 11u);
    x += (x << 15u);
    return x;
}

float rand_float(uint h) {
    uint mantissaMask = 0x007FFFFFu;
    uint f_one = 0x3F800000u;

    h &= mantissaMask;
    h |= f_one;

    float r2 = asfloat(h);
    return r2 - 1.0;
}
