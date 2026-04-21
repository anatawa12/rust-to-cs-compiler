using r2CsRuntime;

namespace r2CsRuntime.Tests;

public class VarTests
{
    [Fact]
    public void Var_DefaultConstructor_UsesDefaultValue()
    {
        var v = new Var<int>();
        Assert.Equal(0, v.f_value);
    }

    [Fact]
    public void Var_ValueConstructor_StoresValue()
    {
        var v = new Var<int>(42);
        Assert.Equal(42, v.f_value);
    }

    [Fact]
    public void Var_FieldCanBeWritten()
    {
        var v = new Var<int>(0);
        v.f_value = 99;
        Assert.Equal(99, v.f_value);
    }

    [Fact]
    public void Var_HoldsReferenceType()
    {
        var v = new Var<string>("hello");
        Assert.Equal("hello", v.f_value);
        v.f_value = "world";
        Assert.Equal("world", v.f_value);
    }

    [Fact]
    public void Var_TwoInstancesAreIndependent()
    {
        var a = new Var<int>(1);
        var b = new Var<int>(2);
        a.f_value = 10;
        Assert.Equal(10, a.f_value);
        Assert.Equal(2, b.f_value);
    }

    [Fact]
    public void Var_StructValue_StoredByValue()
    {
        var v = new Var<(int X, int Y)>((3, 4));
        Assert.Equal(3, v.f_value.X);
        Assert.Equal(4, v.f_value.Y);
        v.f_value = (10, 20);
        Assert.Equal(10, v.f_value.X);
    }

    /// <summary>
    /// Simulates the pattern generated for a closure capturing a local by ref:
    /// the local is moved into a Var and both the original scope and the closure
    /// share access through the same Var object.
    /// </summary>
    [Fact]
    public void Var_SimulatesCapturedLocal()
    {
        var captured = new Var<int>(0);

        // "closure" modifies the var
        Action increment = () => captured.f_value++;

        increment();
        increment();

        Assert.Equal(2, captured.f_value);
    }
}
