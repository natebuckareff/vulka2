struct Vertex
{
    float3 position;
    float _pad0;
    float2 uv;
    float2 _pad1;
}

struct DrawData
{
    float4x4 mvp;
    Vertex *vertices;
    uint *indices;
}

struct DrawBlock
{
    ConstantBuffer<DrawData> draw;
}

struct Material
{
    SamplerState textureSampler;
    Texture2D texture;
}
