using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="LenRef{T}"/> and <see cref="SliceExt"/>.
///
/// <c>LenRef&lt;T&gt;</c> is a general fat reference to any unsized type T.
/// For plain slices (<c>&amp;[T]</c>) the type parameter is
/// <c>Slice&lt;T&gt;</c> and element access goes through <see cref="SliceExt"/>.
/// For DST structs (<c>struct A(Fields, [T])</c>) the type parameter is the
/// outer struct type (e.g. <c>LenRef&lt;s_A&gt;</c>).
/// </summary>
public class LenRefTests
{
    // ── Helpers ───────────────────────────────────────────────────────────

    private sealed class ThreeInts
    {
        public int f_0;
        public int f_1;
        public int f_2;
    }

    private static LenRef<Slice<int>> MakeSlice(ThreeInts obj)
        => RefHelper.FromHeapSlice(obj, ref obj.f_0, 3);

    // ── Heap slice (LenRef<Slice<int>>) ───────────────────────────────────

    [Fact]
    public void HeapSlice_GetElement_ReadsFields()
    {
        var obj   = new ThreeInts { f_0 = 10, f_1 = 20, f_2 = 30 };
        var slice = MakeSlice(obj);

        Assert.Equal(3,  (int)slice.Length);
        Assert.Equal(10, slice.GetElement(0));
        Assert.Equal(20, slice.GetElement(1));
        Assert.Equal(30, slice.GetElement(2));
    }

    [Fact]
    public void HeapSlice_GetElement_WritesFields()
    {
        var obj   = new ThreeInts();
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
        var obj   = new ThreeInts { f_0 = 7 };
        var slice = MakeSlice(obj);
        var r     = slice.GetElementRef(0);
        r.Set(42);
        Assert.Equal(42, obj.f_0);
    }

    [Fact]
    public void HeapSlice_SliceRange_SubRange()
    {
        var obj   = new ThreeInts { f_0 = 1, f_1 = 2, f_2 = 3 };
        var slice = MakeSlice(obj);
        var sub   = slice.SliceRange(1, 2);

        Assert.Equal(2, (int)sub.Length);
        Assert.Equal(2, sub.GetElement(0));
        Assert.Equal(3, sub.GetElement(1));
    }

    [Fact]
    public void HeapSlice_SliceRange_EmptyRange()
    {
        var obj   = new ThreeInts();
        var slice = MakeSlice(obj);
        var empty = slice.SliceRange(0, 0);
        Assert.Equal(0, (int)empty.Length);
    }

    // ── Stack slice ───────────────────────────────────────────────────────

    [Fact]
    public unsafe void StackSlice_GetElement_ReadWrite()
    {
        int[] arr = { 10, 20, 30 };
        fixed (int* ptr = arr)
        {
            var slice = new LenRef<Slice<int>>((nint)ptr, 3);
            Assert.Equal(10, slice.GetElement(0));
            Assert.Equal(20, slice.GetElement(1));
            Assert.Equal(30, slice.GetElement(2));
            slice.GetElement(1) = 99;
        }
        Assert.Equal(99, arr[1]);
    }

    // ── DST struct fat reference (LenRef<TDst>) ───────────────────────────
    // For a Rust DST struct like `struct Buf(i32, [u8])`, a fat reference
    // LenRef<s_Buf> carries the header offset and the tail element count.
    // There is intentionally no element-access on LenRef<TDst> itself —
    // the generated code accesses the tail via SliceExt on a sub-LenRef<Slice<u8>>.

    private sealed class s_Buf_container
    {
        public int f_header;
        #pragma warning disable CS0649
        public byte f_tail_0;
        public byte f_tail_1;
        public byte f_tail_2;
#pragma warning restore CS0649
    }

    [Fact]
    public void DstRef_CarriesLengthAndOffset()
    {
        var obj    = new s_Buf_container { f_header = 42 };
        var r      = RefHelper.FromHeapField(obj, ref obj.f_header);
        var dstRef = new LenRef<s_Buf_container>(obj, r.Offset, 3); // 3 tail bytes

        Assert.Equal(3, (int)dstRef.Length);
        Assert.Same(obj, dstRef.Target);
    }
}
