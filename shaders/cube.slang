struct Vertex
{
    float3 position;
    float _pad0;
    float2 uv;
    float2 _pad1;
};

struct PushConstants
{
    float4x4 mvp;
    Vertex *vertices;
    uint *indices;
    uint textureIndex;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};

[[vk::push_constant]]
ConstantBuffer<PushConstants> pushConstants;

[[vk::binding(0, 0)]]
SamplerState textureSampler;

[[vk::binding(1, 0)]]
Texture2D textures[];

struct VSOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
VSOutput vertexMain(uint vertexId: SV_VertexID)
{
    VSOutput output;
    uint index = pushConstants.indices[vertexId];
    Vertex vertex = pushConstants.vertices[index];
    output.position = mul(float4(vertex.position, 1.0), pushConstants.mvp);
    output.uv = vertex.uv;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VSOutput input) : SV_Target0
{
    return textures[pushConstants.textureIndex].Sample(textureSampler, input.uv);
}
