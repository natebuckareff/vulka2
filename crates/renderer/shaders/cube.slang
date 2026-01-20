#include "cube.inl"

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
    printf("vertexId: %d", vertexId);
    return output;
}

[shader("fragment")]
float4 fragmentMain(VSOutput input) : SV_Target0
{
    return textures[pushConstants.texture_index].Sample(textureSampler, input.uv);
}
