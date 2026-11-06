namespace VrcGetVpm;

/// <summary>
///  Corresponds to char in rust.
/// </summary>
public struct Rune : IEquatable<Rune>
{
    public int Value;

    public static implicit operator Rune(char v) => new Rune { Value = v };

    public RustStr EncodeUtf8() => new(ToBytes(stackalloc byte[4]));

    public ReadOnlySpan<byte> ToBytes(Span<byte> buffer)
    {
        if (Value <= 0x7F)
        {
            buffer[0] = (byte)Value;
            return buffer.Slice(0, 1);
        }
        else if (Value <= 0x7FF)
        {
            buffer[0] = (byte)(((Value >> 6) & 0x1F) | 0xC0);
            buffer[1] = (byte)(((Value >> 0) & 0x3F) | 0x80);
            return buffer.Slice(0, 2);
        }
        else if (Value <= 0xFFFF)
        {
            buffer[0] = (byte)(((Value >> 12) & 0x0F) | 0xE0);
            buffer[1] = (byte)(((Value >> 6) & 0x3F) | 0x80);
            buffer[2] = (byte)(((Value >> 0) & 0x3F) | 0x80);
            return buffer.Slice(0, 3);
        }
        else
        {
            buffer[0] = (byte)(((Value >> 18) & 0x07) | 0xE0);
            buffer[1] = (byte)(((Value >> 12) & 0x3F) | 0x80);
            buffer[2] = (byte)(((Value >> 6) & 0x3F) | 0x80);
            buffer[3] = (byte)(((Value >> 0) & 0x3F) | 0x80);
            return buffer.Slice(0, 4);
        }
    }

    public static bool operator==(Rune a, Rune b) => a.Value == b.Value;
    public static bool operator!=(Rune a, Rune b) => a.Value != b.Value;
    public bool Equals(Rune other) => this == other;
    public override bool Equals(object? obj) => obj is Rune other && Equals(other);
    public override int GetHashCode() => Value.GetHashCode();

    public static Rune Of(int b) => new Rune { Value = b };
}
