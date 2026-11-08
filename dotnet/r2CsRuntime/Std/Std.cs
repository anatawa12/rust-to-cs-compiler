using r2CsRuntime;

namespace VrcGetVpm;

public partial class crt_Std
{
    public static partial class mod_Ffi
    {
        public static partial class mod_OsStr
        {
            public partial class s_OsStr
            {
                public static partial crt_Std.mod_Ffi.mod_OsStr.s_OsStr m_FromEncodedBytesUnchecked(Slice<byte> l_bytes_0) =>
                    s_OsStr.ctor(l_bytes_0);

                public bool m_StartsWith(RustStr l_suffix_0) => m_StartsWith(s_OsStr.m_New(l_suffix_0));
                public bool m_EndsWith(RustStr l_suffix_0) => m_EndsWith(s_OsStr.m_New(l_suffix_0));
            }

            public partial class s_OsString
            {
                public void m_Push(RustStr l_other_0) => m_Push(s_OsStr.m_New(l_other_0));
                public void m_Push(s_OsString l_other_0) => m_Push(l_other_0.m_Deref());

                public static crt_Std.mod_Ffi.mod_OsStr.s_OsString m_From(mod_Path.s_Path l_value_0) =>
                    throw new NotImplementedException(); // No Path implementation ATM
            }
        }
    }
}