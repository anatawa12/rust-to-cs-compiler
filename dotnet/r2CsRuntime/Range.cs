namespace r2CsRuntime;

public struct Range<T>
{
    // start..end
    public static Range<T> NewRange(T start, T end) => throw null;
    // start..=end
    public static Range<T> NewRangeInclusive(T start, T end) => throw null;
    // start..
    public static Range<T> NewRangeFrom(T start) => throw null;
    // ..end
    public static Range<T> NewRangeTo(T end) => throw null;
    // ..=end
    public static Range<T> NewRangeToInclusive(T end) => throw null;
}

public struct RangeFull
{
    // ..
    public static RangeFull NewRangeFull() => throw null;
}

public static class RangeExt
{
    extension(Range<nuint> self)
    {
        public IEnumerator<nuint> GetEnumerator() => throw null;
    }
}
