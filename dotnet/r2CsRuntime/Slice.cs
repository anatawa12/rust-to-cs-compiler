using System.Collections;
using VrcGetVpm;

namespace r2CsRuntime;

public partial struct Slice<T> : IEnumerable<T>
{
    private union Base(T[]);

    private Base _base;
    private int _offset;
    private int _len;

    private Slice(Base @base, int offset, int len)
    {
        _base = @base;
        _offset = offset;
        _len = len;
    }

    public int Length => _len;
    public T this[Index index] => m_Get((nuint)index.ToIndex(Length)).m_Unwrap();

    public static Slice<T> Of(T[] list, int offset, int len) => new Slice<T>(list, offset, len);
    public static implicit operator Slice<T>(T[] list) => new Slice<T>(list, 0, list.Length);

    public bool m_IsEmpty() => _len == 0;
    public nuint m_Len() => (UIntPtr)_len;

    public Memory<T> AsMemory() => _base switch
    {
        T[] ary => ary.AsMemory(_offset, _len),
    };
    public ReadOnlySpan<T> AsSpan() => AsMemory().Span;

    public (T[] array, int offset, int len) AsArrayWithOffsets() => _base switch
    {
        T[] ary => (ary, _offset, _len),
    };

    public IEnumerator<T> GetEnumerator() => m_Iter();

    public IEnumerator<T> m_Iter() => _base switch
    {
        T[] ary => ary.Skip(_offset).Take(_len).GetEnumerator(),
    };
    public crt_Core.mod_Option.s_Option<T> m_First() => this.m_Iter().m_Next();

    public crt_Core.mod_Option.s_Option<T> m_Get(nuint index) =>
        index < (nuint)_len
            ? _base switch
            {
                T[] ary => crt_Core.mod_Option.s_Option<T>.v_Some.ctor(ary[_offset + (int)index])
            }
            : crt_Core.mod_Option.s_Option<T>.v_None.instance;

    public crt_Core.mod_Option.s_Option<T> m_GetMut(nuint index) => m_Get(index);

    public (Slice<T>, Slice<T>) m_SplitAt(nuint index)
    {
        if (index > (nuint)_len) throw  new IndexOutOfRangeException();
        var asInt = (int)index;
        return (
            new Slice<T>(_base, _offset, asInt),
            new Slice<T>(_base, _offset + asInt, _len - asInt)
        );
    }

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

    public Slice<T> Index(Range<nuint> range)
    {
        var (idx, end) = range.GetIndexRage(m_Len());
        return new(_base, _offset + (int)idx, (int)(end - idx));
    }

    public T Index(nuint index) => m_Get(index).m_Unwrap();
    
    public IEnumerator<Slice<T>> m_Windows(nuint windowLen)
    {
        if (windowLen > int.MaxValue) throw new PanicException("Size Exceeds");
        if (windowLen == 0) throw new PanicException("Window Len 0");
        var windowLenInt = (int)windowLen;

        for (int i = 0; i + windowLenInt <= _len; i++)
        {
            yield return new (_base, _offset + i, windowLenInt);
        }
    }
}
