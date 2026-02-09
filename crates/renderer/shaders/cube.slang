#include "cube.inl"

ParameterBlock<DrawBlock> frame;
ParameterBlock<Material> material;

struct VSOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

[shader("vertex")]
VSOutput vertexMain(uint vertexId: SV_VertexID)
{
    VSOutput output;
    uint index = frame.draw.indices[vertexId];
    Vertex vertex = frame.draw.vertices[index];
    output.position = mul(float4(vertex.position, 1.0), frame.draw.mvp);
    output.uv = vertex.uv;
    return output;
}

[shader("fragment")]
float4 fragmentMain(VSOutput input) : SV_Target0
{
    return material.texture.Sample(material.textureSampler, input.uv);
}
