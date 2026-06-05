namespace r2CsRuntime;

public static class Helpers
{
    public static Exception Returns<T>(T value) => new ReturnException<T>(value);
    public static Exception Panic<T>(T value) => new PanicException(value as object);

    /// Runs the lambda expression with ThrowReturns support
    public static T Tr<T>(Func<T> func)
    {
        try
        {
            return func();
        }
        catch (ReturnException<T> e)
        {
            return e.Value;
        }
    }

    public static void Loop(string nama, Action action)
    {
        while (true)
        {
            try
            {
                action();
            }
            catch (BreakException b) when (b.Name == nama)
            {
                break;
            }
            catch (ContinueException b) when (b.Name == nama)
            {
                continue;
            }
        }
    }

    public static T Try<T, R>(VrcGetVpm.crt_Core.mod_Result.s_Result<T, R> result)
    {
        return result switch
        {
            VrcGetVpm.crt_Core.mod_Result.s_Result<T, R>.v_Ok ok => ok.f_0,
            _ => throw new NotImplementedException()
        };
    }
}

internal class ReturnException<T> : Exception
{
    public T Value { get; }

    public ReturnException(T value)
    {
        Value = value;
    }
}

internal class BreakException : Exception
{
    public readonly string Name;
    public BreakException(string name) => Name = name;
}

internal class ContinueException : Exception
{
    public readonly string Name;
    public ContinueException(string name) => Name = name;
}

public class PanicException : Exception
{
    public object? Value { get; }
    public PanicException(object? value) : base(value?.ToString())
    {
        Value = value;
    }
}
