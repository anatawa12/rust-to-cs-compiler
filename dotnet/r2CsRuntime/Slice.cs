using System.Collections;
using VrcGetVpm;

namespace r2CsRuntime;

public partial struct Slice<T> : IEnumerable<T>
{
    private IEnumerable<T> _base;
    private int _offset;
    private int _len;

    private Slice(IEnumerable<T> @base, int offset, int len)
    {
        _base = @base;
        _offset = offset;
        _len = len;
    }

    public int Length => _len;
    public T this[Index index] => throw null;

    public static implicit operator Slice<T>(List<T> list) => new Slice<T>(list, 0, list.Count);
    public static implicit operator Slice<T>(T[] list) => new Slice<T>(list, 0, list.Length);

    public bool m_IsEmpty() => _len == 0;
    public nuint m_Len() => (UIntPtr)_len;
    public IEnumerator<T> GetEnumerator() => m_Iter();
    public List<T> m_ToVec() => new List<T>(_base.Skip(_offset).Take(_len).ToList());

    public IEnumerator<T> m_Iter() => _base.Skip(_offset).Take(_len).GetEnumerator();
    public crt_Core.mod_Option.s_Option<T> m_First() => this.m_Iter().m_Next();

    public crt_Core.mod_Option.s_Option<T> m_Get(nuint index) =>
        index >= (nuint)_len ? crt_Core.mod_Option.s_Option<T>.v_None.instance
        : _base is List<T> list ? crt_Core.mod_Option.s_Option<T>.v_Some.ctor(list[_offset + (int)index])
        : _base is T[] ary ? crt_Core.mod_Option.s_Option<T>.v_Some.ctor(ary[_offset + (int)index])
        : _base.Skip(_offset).Skip((int)index).GetEnumerator().m_Next();

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
}
