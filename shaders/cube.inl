struct Vertex
{
    float3 position;
    float _pad0;
    float2 uv;
    float2 _pad1;
}

struct PushConstants
{
    float4x4 mvp;
    Vertex *vertices;
    uint *indices;
    uint texture_index;
    uint _pad0;
    uint _pad1;
    uint _pad2;
}
