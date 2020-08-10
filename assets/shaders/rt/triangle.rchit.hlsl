struct Payload {
    float3 hitValue;
};

struct Attribute {
    float2 bary;
};

// clang-format off
[shader("closesthit")]
void main(
    inout Payload payload: SV_RayPayload,
    in Attribute attribs: SV_IntersectionAttributes
) {
    // clang-format on
    const float3 barycentrics = float3(
        1.0 - attribs.bary.x - attribs.bary.y, attribs.bary.x, attribs.bary.y);
    payload.hitValue = barycentrics;
}
