struct VertexPacked {
	float4 data0;
};

struct Vertex {
    float3 position;
    float3 normal;
};

float3 unpack_unit_direction_11_10_11(uint pck) {
    return float3(
        float(pck & ((1u << 11u)-1u)) * (2.0f / float((1u << 11u)-1u)) - 1.0f,
        float((pck >> 11u) & ((1u << 10u)-1u)) * (2.0f / float((1u << 10u)-1u)) - 1.0f,
        float((pck >> 21u)) * (2.0f / float((1u << 11u)-1u)) - 1.0f
    );
}

uint floatBitsToUint(float a) {
    return asuint(a);
}

Vertex unpack_vertex(VertexPacked p) {
    Vertex res;
    res.position = p.data0.xyz;
    res.normal = unpack_unit_direction_11_10_11(floatBitsToUint(p.data0.w));
    return res;
}


StructuredBuffer<VertexPacked> vertices;

struct VsOut {
	float4 position: SV_Position;
    [[vk::location(0)]] float4 color: COLOR0;
};

VsOut main(uint vid: SV_VertexID) {
    VsOut vsout;

    Vertex v = unpack_vertex(vertices[vid]);
    vsout.position = float4(v.position * float3(9.0 / 16.0, 1.0, 1.0), 1);
    vsout.color = float4(v.normal * 0.5 + 0.5, 1);

    return vsout;
}
