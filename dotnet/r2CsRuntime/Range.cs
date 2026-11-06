namespace r2CsRuntime;

public struct Range<T>
{
    private readonly T? _lower;
    private readonly T? _upper;
    private readonly byte _bounds;

    public Bound LowerBound => (Bound)(_bounds & 0xF);
    public T Lower => LowerBound != Bound.Unbounded ? _lower! : ThrowUnbounded("lower");

    public Bound UpperBound => (Bound)((_bounds >> 4) & 0xF);
    public T Upper => UpperBound != Bound.Unbounded ? _upper! : ThrowUnbounded("upper");

    static T ThrowUnbounded(string lower) => throw new Exception($"{lower} is unbounded");

    public enum Bound : byte
    {
        Unbounded,
        Inclusive,
        Exclusive,
    }

    private Range(T? lower, Bound lowerBound, T? upper, Bound upperBound)
    {
        _lower = lower;
        _upper = upper;
        _bounds = (byte)((int)lowerBound | (int)upperBound << 4);
    }

    // start..end
    public static Range<T> NewRange(T start, T end) => new(start, Bound.Inclusive, end, Bound.Exclusive);
    // start..=end
    public static Range<T> NewRangeInclusive(T start, T end) => new(start, Bound.Inclusive, end, Bound.Inclusive);
    // start..
    public static Range<T> NewRangeFrom(T start) => new(start, Bound.Inclusive, default, Bound.Unbounded);
    // ..end
    public static Range<T> NewRangeTo(T end) => new(default, Bound.Unbounded, end, Bound.Exclusive);
    // ..=end
    public static Range<T> NewRangeToInclusive(T end) => new (default, Bound.Unbounded, end, Bound.Inclusive);

    public static implicit operator Range<T>(RangeFull full) => new(default, Bound.Unbounded, default, Bound.Unbounded);
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

        internal (nuint begin, nuint end) GetIndexRage(nuint len)
        {
            nuint lower;
            switch (self.LowerBound)
            {
                case Range<nuint>.Bound.Unbounded:
                    lower = 0;
                    break;
                case Range<nuint>.Bound.Inclusive:
                    lower = self.Lower;
                    break;
                case Range<nuint>.Bound.Exclusive:
                    lower = self.Lower + 1;
                    break;
                default:
                    throw new ArgumentOutOfRangeException();
            }
            nuint upper;
            switch (self.UpperBound)
            {
                case Range<nuint>.Bound.Unbounded:
                    upper = len;
                    break;
                case Range<nuint>.Bound.Inclusive:
                    upper = self.Upper + 1;
                    break;
                case Range<nuint>.Bound.Exclusive:
                    upper = self.Upper;
                    break;
                default:
                    throw new ArgumentOutOfRangeException();
            }
            if (lower > len || upper > len) throw new IndexOutOfRangeException();
            if (upper < lower) throw new PanicException("Index inverse.");

            return (lower, upper);
        }
    }
}
