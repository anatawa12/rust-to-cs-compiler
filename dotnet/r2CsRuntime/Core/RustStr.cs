using System.Text;
using r2CsRuntime;

namespace VrcGetVpm;

public struct RustStr : crt_Core.mod_Fmt.t_Display, crt_Core.mod_Fmt.t_Debug, crt_Core.mod_Cmp.t_Ord<RustStr>
{
    private Slice<byte> _slice;

    public ReadOnlySpan<byte> Inner => _slice.AsSpan();

    private RustStr(Slice<byte> data) => _slice = data;
    public RustStr(ReadOnlySpan<byte> data)
    {
        _slice = (Slice<byte>)data.ToArray();
    }

    internal static RustStr FromUtf8Unsafe(Slice<byte> data) => new(data);

    public nuint m_Len() => _slice.m_Len();
    public bool m_IsEmpty() => _slice.m_IsEmpty();
    public IEnumerator<byte> m_Bytes() => _slice.m_Iter();
    public Slice<byte> m_AsBytes() => _slice;
    public RustStr m_Trim() => new(TrimStart(TrimEnd(_slice)));
    public RustStr m_TrimStart() => new(TrimStart(_slice));

    private static Slice<byte> TrimStart(Slice<byte> s)
    {
        while (true)
        {
            int len = s switch
            {
                [0x20 or (>= 0x09 and <= 0x0D), ..] => 1,                               // U+0020, U+0009-000D
                [0xC2, 0x85 or 0xA0, ..] => 2,                                          // U+0085, U+00A0
                [0xE1, 0x9A, 0x80, ..] => 3,                                            // U+1680
                [0xE2, 0x80, (>= 0x80 and <= 0x8A) or 0xA8 or 0xA9 or 0xAF, ..] => 3,   // U+2000-200A, U+2028, U+2029, U+202F
                [0xE2, 0x81, 0x9F, ..] => 3,                                            // U+205F
                [0xE3, 0x80, 0x80, ..] => 3,                                            // U+3000
                _ => 0,
            };
            if (len == 0) return s;
            s = s.Index(Range<nuint>.NewRangeFrom((nuint)len));
        }
    }

    private static Slice<byte> TrimEnd(Slice<byte> s)
    {
        while (true)
        {
            int len = s switch
            {
                [.., 0x20 or (>= 0x09 and <= 0x0D)] => 1,                               // U+0020, U+0009-000D
                [.., 0xC2, 0x85 or 0xA0] => 2,                                          // U+0085, U+00A0
                [.., 0xE1, 0x9A, 0x80] => 3,                                            // U+1680
                [.., 0xE2, 0x80, (>= 0x80 and <= 0x8A) or 0xA8 or 0xA9 or 0xAF] => 3,   // U+2000-200A, U+2028, U+2029, U+202F
                [.., 0xE2, 0x81, 0x9F] => 3,                                            // U+205F
                [.., 0xE3, 0x80, 0x80] => 3,                                            // U+3000
                _ => 0,
            };
            if (len == 0) return s;
            s = s.Index(Range<nuint>.NewRangeTo(s.m_Len() - (nuint)len));
        }
    }

    public crt_Core.mod_Option.s_Option<RustStr> m_StripPrefix(RustStr prefix) =>
        this.m_Len() >= prefix.m_Len() && this._slice.Index(Range<nuint>.NewRange(0, prefix.m_Len())).SequenceEqual(prefix._slice)
            ? crt_Core.mod_Option.s_Option<RustStr>.v_Some.ctor(new RustStr(this._slice.Index(Range<nuint>.NewRangeFrom(prefix.m_Len()))))
            : crt_Core.mod_Option.s_Option<RustStr>.v_None.instance;

    public crt_Core.mod_Option.s_Option<(RustStr, RustStr)> m_SplitOnce(Rune separator) => SplitOnceImpl(separator.ToBytes(stackalloc byte[4]));
    public crt_Core.mod_Option.s_Option<(RustStr, RustStr)> m_SplitOnce(RustStr separator) => SplitOnceImpl(separator.Inner);
    private crt_Core.mod_Option.s_Option<(RustStr, RustStr)> SplitOnceImpl(ReadOnlySpan<byte> separator)
    {
        var index = Inner.IndexOf(separator);
        if (index < 0) return crt_Core.mod_Option.s_Option<(RustStr, RustStr)>.v_None.instance;
        return crt_Core.mod_Option.s_Option<(RustStr, RustStr)>.v_Some.ctor((
            new RustStr(_slice.Index(Range<nuint>.NewRangeTo((nuint)index))), 
            new RustStr(_slice.Index(Range<nuint>.NewRangeFrom((nuint)(index + separator.Length))))));
    }

    public System.Collections.Generic.IEnumerator<RustStr> m_Split(Rune separator) => SplitImpl(separator.ToBytes(stackalloc byte[4]).ToArray());
    public System.Collections.Generic.IEnumerator<RustStr> m_Split(RustStr separator) => SplitImpl(separator._slice.AsMemory());
    private System.Collections.Generic.IEnumerator<RustStr> SplitImpl(Memory<byte> separator)
    {
        var begin = 0;
        for(;;)
        {
            var index = Inner.Slice(begin).IndexOf(separator.Span);
            if (index < 0) break;
            yield return new RustStr(Inner.Slice(begin, index - begin));
            begin = index + separator.Length;
        }
        yield return new RustStr(Inner.Slice(begin));
    }

    public (RustStr, RustStr) m_SplitAt(nuint at)
    {
        if (at != m_Len() && !IsUtf8StartByte(Inner[(int)at])) throw new PanicException("Begin Not At Char Boundary.");
        return (new RustStr(Inner.Slice(0, (int)at)), new RustStr(Inner.Slice((int)at)));
    }

    public RustStr Index(Range<nuint> range)
    {
        var (begin, end) = range.GetIndexRage(m_Len());
        if (begin != m_Len() && !IsUtf8StartByte(Inner[(int)begin])) throw new PanicException("Begin Not At Char Boundary.");
        if (end != m_Len() && !IsUtf8StartByte(Inner[(int)end])) throw new PanicException("End Not At Char Boundary.");
        return new RustStr(_slice.Index(range));
    }

    public IEnumerator<Rune> m_Chars() => CharsWithIndex().m_Map(x => x.Item2);

    internal IEnumerator<(int index, Rune rune)> CharsWithIndex()
    {
        for (int i = 0; (nuint)i < m_Len(); )
        {
            yield return (i, Utf8Utility.GetRune(Inner, ref i));
        }
    }

    private static bool IsUtf8StartByte(byte b) => (sbyte)b >= -64;

    // TODO: This should be better value
    public RustStr m_AsRef() => this;

    // Box<RustStr>
    public RustStr m_Clone() => this;
    public RustStr m_IntoBoxedStr() => this;

    public bool m_Contains(Rune c) => Inner.IndexOf(c.ToBytes(stackalloc byte[4])) >= 0;
    public crt_Core.mod_Option.s_Option<nuint> m_Find(Func<Rune, bool> cond) =>
        CharsWithIndex()
            .m_FilterMap(c => cond(c.rune) ? crt_Core.mod_Option.s_Option<nuint>.v_Some.ctor((nuint)c.index) : crt_Core.mod_Option.s_Option<nuint>.v_None.instance)
            .m_Next();
    public crt_Core.mod_Option.s_Option<nuint> m_Find(Rune[] c)
    {
        Span<byte> buffer = stackalloc byte[4];

        var index = -1;
        foreach (var rune in c)
        {
            var thisFound = Inner.IndexOf(rune.ToBytes(buffer));
            if (thisFound >= 0) index = (int)Math.Min((uint)thisFound, (uint)buffer.Length);
        }

        return index >= 0
            ? crt_Core.mod_Option.s_Option<nuint>.v_Some.ctor((nuint)index)
            : crt_Core.mod_Option.s_Option<nuint>.v_None.instance;
    }

    public bool m_Eq(RustStr p_0) => this == p_0;
    public override bool Equals(object? obj) => obj is RustStr other && this == other;

    public crt_Core.mod_Option.s_Option<crt_Core.mod_Cmp.s_Ordering> m_PartialCmp(RustStr other) =>
        crt_Core.mod_Option.s_Option<crt_Core.mod_Cmp.s_Ordering>.v_Some.ctor(m_Cmp(other));

    public crt_Core.mod_Cmp.s_Ordering m_Cmp(RustStr p_0) => this.Inner.SequenceCompareTo(p_0.Inner) switch
    {
        < 0 => crt_Core.mod_Cmp.s_Ordering.v_Less.instance,
        0 => crt_Core.mod_Cmp.s_Ordering.v_Equal.instance,
        > 0 => crt_Core.mod_Cmp.s_Ordering.v_Greater.instance,
    };

    public override string ToString()
    {
        var (array, offset, len) = _slice.AsArrayWithOffsets();
        return Encoding.UTF8.GetString(array, offset, len);
    }

    public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DisplayFmt(crt_Core.mod_Fmt.s_Formatter p_0) => p_0.m_WriteStr(this);
    public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DebugFmt(crt_Core.mod_Fmt.s_Formatter p_0) => p_0.m_WriteStr(this); // TODO: quote

    public static bool operator==(RustStr a, RustStr b) => a.Inner.SequenceEqual(b.Inner);
    public static bool operator !=(RustStr a, RustStr b) => !(a == b);

    public nuint m_AsPtr() => throw null; // TODO: remove
}

static class Utf8Utility
{
    public static Rune GetRune(ReadOnlySpan<byte> slice, ref int index)
    {
        var len = Utf8Len(slice[index]);
        var result = len switch
        {
            1 => Rune.Of(slice[index]),
            2 => Rune.Of(((slice[index] & 0x1F) << 6) | ((slice[index] & 0x3F) << 0)),
            3 => Rune.Of(((slice[index] & 0x0F) << 12) | ((slice[index] & 0x3F) << 6) | ((slice[index] & 0x3F) << 0)),
            4 => Rune.Of(((slice[index] & 0x07) << 18) | ((slice[index] & 0x3F) << 12) | ((slice[index] & 0x3F) << 6) | ((slice[index] & 0x3F) << 0)),
        };
        index += len;
        return result;
    }

    public static int Utf8Len(byte first) => first switch
    {
        < 0x80 => 1,
        < 0xE0 => 2,
        < 0xF0 => 3,
        _ => 4
    };
}
