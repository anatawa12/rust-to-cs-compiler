using System.Collections;

namespace r2CsRuntime;

public struct Slice<T> : IEnumerable<T>
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

    public static implicit operator Slice<T>(List<T> list) => new Slice<T>(list, 0, list.Count);
    public static implicit operator Slice<T>(T[] list) => new Slice<T>(list, 0, list.Length);

    public bool m_IsEmpty() => _len == 0;
    public nuint m_Len() => (UIntPtr)_len;
    public IEnumerator<T> GetEnumerator() => m_Iter();
    public List<T> m_ToVec() => new List<T>(_base.Skip(_offset).Take(_len).ToList());

    public IEnumerator<T> m_Iter() => _base.Skip(_offset).Take(_len).GetEnumerator();

    IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
}
