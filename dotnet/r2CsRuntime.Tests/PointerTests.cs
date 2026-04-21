using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="Pointer{T}"/>, <see cref="LenPointer{T}"/>, and
/// <see cref="DynPointer{TVtable}"/>.
/// </summary>
public class PointerTests
{
    // ── Pointer<T> ────────────────────────────────────────────────────────

    private sealed class IntBox { public int f_value; }

    [Fact]
    public void Pointer_NullConstant_IsNull()
    {
        var p = Pointer<int>.Null;
        Assert.True(p.IsNull());
        Assert.Null(p.Target);
        Assert.Equal(0, p.Offset);
    }

    [Fact]
    public void Pointer_HeapDeref_ReadWrite()
    {
        var box = new IntBox { f_value = 42 };
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        var p = new Pointer<int>(box, r.Offset);

        Assert.Equal(42, p.Deref());
        p.Deref() = 100;
        Assert.Equal(100, box.f_value);
    }

    [Fact]
    public void Pointer_AsRef_RoundTrips()
    {
        var box = new IntBox { f_value = 7 };
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        var p = new Pointer<int>(box, r.Offset);

        var r2 = p.AsRef();
        r2.Set(55);
        Assert.Equal(55, box.f_value);
    }

    [Fact]
    public unsafe void Pointer_Add_AdvancesByElementSize()
    {
        int[] arr = { 10, 20, 30 };
        fixed (int* ptr = arr)
        {
            var p = new Pointer<int>((nint)ptr);
            Assert.Equal(10, p.Deref());
            Assert.Equal(20, p.Add(1).Deref());
            Assert.Equal(30, p.Add(2).Deref());
        }
    }

    // ── LenPointer<T> ─────────────────────────────────────────────────────

    private sealed class ThreeInts { public int f_0, f_1, f_2; }

    [Fact]
    public void LenPointer_NullConstant_HasZeroLength()
    {
        var p = LenPointer<int>.Null;
        Assert.Equal(0, p.Length);
    }

    [Fact]
    public void LenPointer_HeapDeref_ReadWrite()
    {
        var obj = new ThreeInts { f_0 = 1, f_1 = 2, f_2 = 3 };
        var lenRef = RefHelper.FromHeapLenRef(obj, ref obj.f_0, 3);
        var p = new LenPointer<int>(lenRef.Target!, lenRef.Offset, 3);

        Assert.Equal(1, p.Deref(0));
        Assert.Equal(2, p.Deref(1));
        Assert.Equal(3, p.Deref(2));

        p.Deref(1) = 99;
        Assert.Equal(99, obj.f_1);
    }

    [Fact]
    public void LenPointer_AsRef_ConvertedToLenRef()
    {
        var obj = new ThreeInts { f_0 = 5, f_1 = 6, f_2 = 7 };
        var lenRef = RefHelper.FromHeapLenRef(obj, ref obj.f_0, 3);
        var p = new LenPointer<int>(lenRef.Target!, lenRef.Offset, 3);

        var converted = p.AsRef();
        Assert.Equal(5, converted.GetElement(0));
    }

    // ── DynPointer<TVtable> ───────────────────────────────────────────────

    private readonly struct EmptyVtable { }

    [Fact]
    public void DynPointer_AsRef_ConvertedToDynRef()
    {
        var box = new IntBox { f_value = 11 };
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        var dp = new DynPointer<EmptyVtable>(box, r.Offset, default);

        var dynRef = dp.AsRef();
        Assert.Same(box, dynRef.Target);
        Assert.Equal(r.Offset, dynRef.Offset);
    }

    [Fact]
    public void DynPointer_AsVoidPointer_ErasesType()
    {
        var box = new IntBox { f_value = 0 };
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        var dp = new DynPointer<EmptyVtable>(box, r.Offset, default);

        var vp = dp.AsVoidPointer();
        Assert.Same(box, vp.Target);
        Assert.Equal(r.Offset, vp.Offset);
    }
}
