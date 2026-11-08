using System.Text;

namespace VrcGetVpm;

public static partial class crt_Alloc
{
    public static partial class mod_String
    {
        public partial class s_String : crt_Core.mod_Fmt.t_Display, crt_Core.mod_Fmt.t_Debug
        {
            public partial RustStr m_AsStr() => RustStr.FromUtf8Unsafe(f_0.m_AsSlice()); // TODO: optimize

            public static explicit operator s_String(string cast) => new s_String { f_0 = new mod_Vec.s_Vec<byte>(Encoding.UTF8.GetBytes(cast)) };

            public RustStr m_IntoBoxedStr() => m_AsStr();

            // str functions
            public RustStr m_AsRef() => m_AsStr().m_AsRef();
            public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DisplayFmt(crt_Core.mod_Fmt.s_Formatter p_0) => m_AsStr().DisplayFmt(p_0);
            public crt_Core.mod_Result.s_Result<ValueTuple, crt_Core.mod_Fmt.s_Error> DebugFmt(crt_Core.mod_Fmt.s_Formatter p_0) => m_AsStr().DebugFmt(p_0);
            public RustStr m_Deref() => m_AsStr();
            public void m_Push(Rune rune) => m_PushStr(rune.EncodeUtf8());
        }
    }
}

public static partial class RustStrExtensions
{
    extension(RustStr str)
    {
        public crt_Alloc.mod_String.s_String m_Replace(Rune from, RustStr to) => 
            str.Replace(from.ToBytes(stackalloc byte[4]), to.Inner);

        public crt_Alloc.mod_String.s_String m_ToOwned() => new crt_Alloc.mod_String.s_String()
            { f_0 = new crt_Alloc.mod_Vec.s_Vec<byte>(str.Inner.ToArray()) };

        public static RustStr m_From(crt_Alloc.mod_String.s_String s) => s.m_AsStr();
        public static RustStr m_From(RustStr s) => s;

        private crt_Alloc.mod_String.s_String Replace(ReadOnlySpan<byte> from, ReadOnlySpan<byte> to)
        {
            var data = str.Inner;

            if (from.Length == 0)
            {
                // Special case: "": insert 'to' to every character boundary
                var result1 = crt_Alloc.mod_Vec.s_Vec<byte>.m_WithCapacity((nuint)(data.Length * (1 + to.Length)));
                for (var index = 0; index < data.Length; )
                {
                    result1.ExtendFromSpan(to);
                    var len = Utf8Utility.Utf8Len(data[index]);
                    result1.ExtendFromSpan(data.Slice(index, len));
                    index += len;
                }

                result1.ExtendFromSpan(to);

                return new crt_Alloc.mod_String.s_String { f_0 = result1 };
            }
            else
            {
                var result1 = crt_Alloc.mod_Vec.s_Vec<byte>.m_WithCapacity((nuint)(data.Length * (1 + to.Length)));

                for (;;)
                {
                    var index = data.IndexOf(from);
                    if (index < 0) break;
                    result1.m_Reserve((nuint)(index + to.Length));

                    result1.ExtendFromSpan(data.Slice(0, index));
                    data = data.Slice(index + from.Length);

                    result1.ExtendFromSpan(to);
                }

                result1.ExtendFromSpan(data);

                return new crt_Alloc.mod_String.s_String { f_0 = result1 };
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
        public crt_Core.mod_Option.s_Option<RustStr> m_AsDeref() => self.m_Map(x => x.m_AsStr());
    }

    extension(string self)
    {
        // TODO: remove this
        public crt_Alloc.mod_String.s_String m_AsRef() => (crt_Alloc.mod_String.s_String)(self);
    }
}
