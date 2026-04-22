using System.Runtime.CompilerServices;
using r2CsRuntime;

namespace r2CsRuntime.Tests;

/// <summary>
/// Tests for <see cref="Ref{T}"/> and <see cref="RefHelper"/>.
///
/// The tests use classes as the "owning heap object" so that
/// <see cref="RefHelper.FromHeapField"/> can compute byte offsets.
/// </summary>
public class RefTests
{
    // ── Test fixtures (classes so they live on the heap) ──────────────────

    private sealed class IntBox
    {
        public int f_value;
        public IntBox(int v) => f_value = v;
    }

    private sealed class TwoInts
    {
        public int f_x;
        public int f_y;
    }

    private sealed class StringBox
    {
        public string f_value = "";
    }

    // ── Heap ref: read ────────────────────────────────────────────────────

    [Fact]
    public void HeapRef_Read_ReturnsFieldValue()
    {
        var box = new IntBox(42);
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        Assert.Equal(42, r.Get());
    }

    [Fact]
    public void HeapRef_AsRef_AliasesField()
    {
        var box = new IntBox(10);
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        // write through the ref
        r.AsRef() = 99;
        Assert.Equal(99, box.f_value);
    }

    [Fact]
    public void HeapRef_Set_WritesFieldValue()
    {
        var box = new IntBox(0);
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        r.Set(77);
        Assert.Equal(77, box.f_value);
    }

    [Fact]
    public void HeapRef_DirectFieldMutation_ReflectedInRef()
    {
        var box = new IntBox(5);
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        box.f_value = 50;
        Assert.Equal(50, r.Get());
    }

    // ── Heap ref: second field ────────────────────────────────────────────

    [Fact]
    public void HeapRef_SecondField_IndependentFromFirst()
    {
        var two = new TwoInts { f_x = 1, f_y = 2 };
        var rx = RefHelper.FromHeapField(two, ref two.f_x);
        var ry = RefHelper.FromHeapField(two, ref two.f_y);

        rx.Set(10);
        ry.Set(20);

        Assert.Equal(10, two.f_x);
        Assert.Equal(20, two.f_y);
        Assert.Equal(10, rx.Get());
        Assert.Equal(20, ry.Get());
    }

    [Fact]
    public void HeapRef_TwoRefs_DoNotAlias()
    {
        var two = new TwoInts { f_x = 0, f_y = 0 };
        var rx = RefHelper.FromHeapField(two, ref two.f_x);
        var ry = RefHelper.FromHeapField(two, ref two.f_y);

        rx.Set(100);
        Assert.Equal(0, ry.Get()); // ry is unaffected
    }

    // ── Heap ref: reference type field ────────────────────────────────────

    [Fact]
    public void HeapRef_StringField_ReadWrite()
    {
        var box = new StringBox { f_value = "hello" };
        var r = RefHelper.FromHeapField(box, ref box.f_value);
        Assert.Equal("hello", r.Get());
        r.Set("world");
        Assert.Equal("world", box.f_value);
    }

    // ── Var<T> integration ────────────────────────────────────────────────

    [Fact]
    public void FromVar_ReadsVarValue()
    {
        var v = new Var<int>(55);
        var r = RefHelper.FromVar(v);
        Assert.Equal(55, r.Get());
    }

    [Fact]
    public void FromVar_WritesBackToVar()
    {
        var v = new Var<int>(0);
        var r = RefHelper.FromVar(v);
        r.Set(42);
        Assert.Equal(42, v.f_value);
    }

    [Fact]
    public void FromVar_RefAndDirectAccessAlias()
    {
        var v = new Var<int>(1);
        var r = RefHelper.FromVar(v);
        v.f_value = 99;
        Assert.Equal(99, r.Get());
        r.Set(7);
        Assert.Equal(7, v.f_value);
    }

    // ── Stack ref ─────────────────────────────────────────────────────────

    [Fact]
    public unsafe void StackRef_ReadWrite()
    {
        // Use a single-element array pinned via fixed so the address is stable.
        int[] buf = { 123 };
        Ref<int> r;
        fixed (int* ptr = buf)
        {
            r = RefHelper.FromStackPointer<int>(ptr);
            Assert.Equal(123, r.Get());
            r.Set(456);
        }
        Assert.Equal(456, buf[0]);
    }

    // ── Structural equality ───────────────────────────────────────────────

    [Fact]
    public void HeapRef_SameFieldYieldsSameOffsets()
    {
        var box = new IntBox(1);
        var r1 = RefHelper.FromHeapField(box, ref box.f_value);
        var r2 = RefHelper.FromHeapField(box, ref box.f_value);
        Assert.Equal(r1.Target, r2.Target);
        Assert.Equal(r1.Offset, r2.Offset);
    }
}
