using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="LenRef{T}"/>.
/// </summary>
public class LenRefTests
{
    // ── Helper: a heap object that owns an inline array ───────────────────

    /// <summary>
    /// Simulates a struct with three consecutive int fields, as would be
    /// generated for a fixed-size inline array in a Rust struct.
    /// </summary>
    private sealed class ThreeInts
    {
        public int f_0;
        public int f_1;
        public int f_2;
    }

    // ── Heap slice construction ───────────────────────────────────────────

    private static LenRef<int> MakeSlice(ThreeInts obj)
    {
        // Simulate: &mut obj.f_0 as &mut [int; 3] by creating a LenRef
        // starting at the first field with length 3.
        // In generated code the compiler would compute the offset once via
        // Unsafe.ByteOffset.
        return RefHelper.FromHeapLenRef(obj, ref obj.f_0, 3);
    }

    [Fact]
    public void HeapSlice_GetElement_ReadsFields()
    {
        var obj = new ThreeInts { f_0 = 10, f_1 = 20, f_2 = 30 };
        var slice = MakeSlice(obj);

        Assert.Equal(3, (int)slice.Length);
        Assert.Equal(10, slice.GetElement(0));
        Assert.Equal(20, slice.GetElement(1));
        Assert.Equal(30, slice.GetElement(2));
    }

    [Fact]
    public void HeapSlice_GetElement_WritesFields()
    {
        var obj = new ThreeInts { f_0 = 0, f_1 = 0, f_2 = 0 };
        var slice = MakeSlice(obj);

        slice.GetElement(0) = 100;
        slice.GetElement(1) = 200;
        slice.GetElement(2) = 300;

        Assert.Equal(100, obj.f_0);
        Assert.Equal(200, obj.f_1);
        Assert.Equal(300, obj.f_2);
    }

    [Fact]
    public void HeapSlice_GetElementRef_AliasesField()
    {
        var obj = new ThreeInts { f_0 = 7 };
        var slice = MakeSlice(obj);
        var r = slice.GetElementRef(0);
        r.Set(42);
        Assert.Equal(42, obj.f_0);
    }

    [Fact]
    public void HeapSlice_Slice_SubRange()
    {
        var obj = new ThreeInts { f_0 = 1, f_1 = 2, f_2 = 3 };
        var slice = MakeSlice(obj);
        var sub = slice.Slice(1, 2);

        Assert.Equal(2, (int)sub.Length);
        Assert.Equal(2, sub.GetElement(0));
        Assert.Equal(3, sub.GetElement(1));
    }

    [Fact]
    public void HeapSlice_Slice_EmptyRange()
    {
        var obj = new ThreeInts();
        var slice = MakeSlice(obj);
        var empty = slice.Slice(0, 0);
        Assert.Equal(0, (int)empty.Length);
    }

    // ── Stack slice ───────────────────────────────────────────────────────

    [Fact]
    public unsafe void StackSlice_GetElement_ReadWrite()
    {
        int a = 1, b = 2, c = 3;
        // These three locals happen to be consecutive on the stack in debug
        // builds, but that is NOT guaranteed. Use an array instead.
        int[] arr = { 10, 20, 30 };
        LenRef<int> slice;
        fixed (int* ptr = arr)
        {
            slice = new LenRef<int>((nint)ptr, 3);
            Assert.Equal(10, slice.GetElement(0));
            Assert.Equal(20, slice.GetElement(1));
            Assert.Equal(30, slice.GetElement(2));

            slice.GetElement(1) = 99;
        }
        Assert.Equal(99, arr[1]);
        _ = a; _ = b; _ = c; // suppress unused warnings
    }
}
