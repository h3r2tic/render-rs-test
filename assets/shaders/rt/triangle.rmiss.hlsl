struct Payload {
    float3 hitValue;
};

// clang-format off
[shader("miss")]
void main(inout Payload payload : SV_RayPayload) {
    // clang-format on
    payload.hitValue = float3(0.0, 0.1, 0.3);
}
