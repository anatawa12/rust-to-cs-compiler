using System.Text;

namespace VrcGetVpm;

public static partial class crt_Alloc
{
    public static class mod_String
    {
        public class s_String : RustStr
        {
            public static s_String m_New() => new s_String(Array.Empty<byte>());

            internal s_String(byte[] data) : base(data, data.Length) { }
            internal s_String(byte[] data, int length) : base(data, length)
            {
            }

            public RustStr m_AsStr() => this;

            public void m_Clear()
            {
                length = 0;
            }

            public static s_String m_Default() => m_New();

            public static explicit operator s_String(string cast) => new s_String(Encoding.UTF8.GetBytes(cast));
        }
    }
}

public static partial class RustStrExtensions
{
    extension(RustStr str)
    {
        public crt_Alloc.mod_String.s_String m_Replace(Rune from, RustStr to) => 
            str.Replace(from.ToBytes(stackalloc byte[4]), to.Inner);

        public crt_Alloc.mod_String.s_String m_ToOwned() => new(str.Inner.ToArray());

        public static RustStr m_From(crt_Alloc.mod_String.s_String s) => s;
        public static RustStr m_From(RustStr s) => s;
        public static RustStr m_From(string s) => (crt_Alloc.mod_String.s_String)s; // TODO: remove

        private crt_Alloc.mod_String.s_String Replace(ReadOnlySpan<byte> from, ReadOnlySpan<byte> to)
        {
            var data = str.Inner;

            if (from.Length == 0)
            {
                // Special case: "": insert 'to' to every character boundary
                byte[] result = new byte[data.Length * (1 + to.Length)];
                var resultLen = 0;
                for (var index = 0; index < data.Length; )
                {
                    to.CopyTo(result.AsSpan(resultLen));
                    resultLen += to.Length;
                    var len = Utf8Utility.Utf8Len(data[index]);
                    data.Slice(index, len).CopyTo(result.AsSpan(resultLen));
                    resultLen += len;
                    index += len;
                }

                {
                    to.CopyTo(result.AsSpan(resultLen));
                    resultLen += to.Length;
                }

                return new crt_Alloc.mod_String.s_String(result, resultLen);
            }
            else
            {

                byte[] result = new byte[Math.Max(data.Length, data.Length - from.Length + to.Length)];
                int resultLen = 0;

                for (;;)
                {
                    var index = data.IndexOf(from);
                    if (index < 0) break;
                    var newLen = resultLen + index + to.Length;

                    if (newLen > result.Length)
                    {
                        // extend
                        var newBuf = new byte[Math.Max(result.Length * 2, newLen)];
                        result.CopyTo(newBuf);
                        result = newBuf;
                    }

                    data.Slice(0, index).CopyTo(result.AsSpan(resultLen));
                    data = data.Slice(index + from.Length);

                    to.CopyTo(result.AsSpan(resultLen + index));
                    resultLen = newLen;
                }

                {
                    var newLen = resultLen + data.Length;

                    if (newLen > result.Length)
                    {
                        // extend
                        var newBuf = new byte[Math.Max(result.Length * 2, newLen)];
                        result.CopyTo(newBuf);
                        result = newBuf;
                    }

                    data.CopyTo(result.AsSpan(resultLen));
                    resultLen = newLen;
                }

                return new crt_Alloc.mod_String.s_String(result, resultLen);
            }
        }
    }

    // Box<RustStr>
    extension(crt_Core.mod_Option.s_Option<RustStr> self)
    {
        public crt_Core.mod_Option.s_Option<RustStr> m_AsDeref() => self;
    }

    extension(crt_Core.mod_Option.s_Option<crt_Alloc.mod_String.s_String> self)
    {
        public crt_Core.mod_Option.s_Option<RustStr> m_AsDeref() => self.m_Map(RustStr (x) => x);
    }

    extension(string self)
    {
        // TODO: remove this
        public crt_Alloc.mod_String.s_String m_AsRef() => (crt_Alloc.mod_String.s_String)(self);
    }
}
