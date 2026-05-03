namespace r2CsRuntime;

/// <summary>
/// Wraps a value to allow partial-move / drop tracking.
/// All Rust local variables and struct fields are represented as Slot&lt;T&gt;.
/// </summary>
public class Slot<T>
{
    public T value;
    /// <summary>Set to true once the slot has been dropped / moved out.</summary>
    public bool dropped;

    public Slot(T value) => this.value = value;

    public static implicit operator Slot<T>(T value) => new Slot<T>(value);
}
