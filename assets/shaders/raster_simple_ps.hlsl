struct PsIn {
    [[vk::location(0)]] float4 color : COLOR0;
};

// clang-format off
float4 main(PsIn ps): SV_TARGET {
    // clang-format on
    return ps.color;
}
