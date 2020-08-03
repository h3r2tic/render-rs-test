struct VsOut {
	float4 position: SV_Position;
    [[vk::location(0)]] float4 color: COLOR0;
};

VsOut main(uint vid: SV_VertexID) {
    VsOut vsout;

    const float TAU = 6.28318530717958647692528676655900577;
    float a = vid * TAU / 3;
    vsout.position = float4(cos(a) / (16.0 / 9.0) * 0.5, sin(a) * 0.5, 0, 1);
    vsout.color = float4(vid == 0, vid == 1, vid == 2, 1.0);

    return vsout;
}
