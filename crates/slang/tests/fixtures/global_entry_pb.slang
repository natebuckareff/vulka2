struct Foo
{
    float4 dummy;
}

ParameterBlock<Foo> global;

[shader("compute")]
void main(ParameterBlock<Foo> ep)
{
}
