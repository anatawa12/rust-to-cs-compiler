namespace System.Runtime.CompilerServices
{
    sealed class AsyncMethodBuilderAttribute : Attribute
    {
        public AsyncMethodBuilderAttribute(Type builderType)
        {
            BuilderType = builderType;
        }

        public Type BuilderType { get; }
    }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, AllowMultiple = false)]
    public class UnionAttribute : Attribute {}
    public class RequiredMemberAttribute : Attribute {}

    public class CompilerFeatureRequiredAttribute : Attribute
    {
        public CompilerFeatureRequiredAttribute(string featureName)
        {
            
        }
    }

    public interface IUnion
    {
        public object? Value { get; }
    }
}

namespace System
{
    public struct Index
    {
        private readonly int _value;
        public bool IsFromEnd => _value < 0;
        public int Value => _value < 0 ? ~_value : _value;

        public Index(int value, bool fromEnd = false)
        {
            if (value < 0)
            {
                ThrowOutOfRange();
                static void ThrowOutOfRange() => throw new ArgumentOutOfRangeException(nameof(value));
            }

            _value = fromEnd ? ~value : value;
        }

        public int ToIndex(int length) => IsFromEnd ? length - Value : Value; 
        public int GetOffset(int length)
        {
            // https://github.com/dotnet/dotnet/blob/b0f34d51fccc69fd334253924abd8d6853fad7aa/src/runtime/src/libraries/System.Private.CoreLib/src/System/Index.cs#L109-L121
            int offset = _value;
            if (IsFromEnd)
            {
                offset += length + 1;
            }
            return offset;
        }
    }
}
