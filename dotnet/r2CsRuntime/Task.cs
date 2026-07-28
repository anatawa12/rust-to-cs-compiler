using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;

namespace r2CsRuntime;

/// <summary>
/// The type represents Future in Rust.
/// </summary>
/// <typeparam name="TResult"></typeparam>
[AsyncMethodBuilder(typeof(RustTaskBuilder<>))]
public class RustTask<TResult>
{
    internal TaskState State;
    internal IAsyncStateMachine? StateMachine { get; set; }

    internal TResult? Result;
    internal ExceptionDispatchInfo? Exception;

    internal enum TaskState
    {
        Created,
        Running,
        CompletedSuccessfully,
        Failed,
        Cancelled,
    }

    public RustAwaiter<TResult> GetAwaiter()
    {
        if (State == TaskState.Created)
        {
            // We start task on await-ing task so we start here
            StartTask();
        }

        return new RustAwaiter<TResult>(this);
    }

    public void Cancel()
    {
        if (IsCompleted) return; // Nothing to do if completed or canceled
        if (StateMachine == null) throw new InvalidOperationException("Task is not initialized");
        State = TaskState.Cancelled;
        _awaitingTaskAwaiter?.Cancel();
        if (StateMachine != null) throw new InvalidOperationException("Task does not cancelled");
        RunCallOnCompleted();
    }

    private Action? _onCompleted;
    private IRustAwaiter? _awaitingTaskAwaiter;

    #region Awaiter Implementation

    internal bool IsCompleted => State is TaskState.CompletedSuccessfully or TaskState.Failed or TaskState.Cancelled;

    internal void OnCompleted(Action completion)
    {
        if (_onCompleted != null) throw new InvalidOperationException("Task already has an onCompleted method");
        _onCompleted = completion;
    }

    internal TResult GetResult()
    {
        switch (State)
        {
            case TaskState.Created:
            case TaskState.Running:
                throw new InvalidOperationException("Tha tasks is not completed yet");
            case TaskState.CompletedSuccessfully:
                return Result!;
            case TaskState.Failed:
                Exception!.Throw();
                throw new InvalidOperationException("Unreachable");
            case TaskState.Cancelled:
                throw RustCancellationException.Instance;
            default:
                throw new InvalidOperationException("Unreachable");
        }
    }

    #endregion

    private void StartTask()
    {
        if (StateMachine == null) throw new InvalidOperationException("Task is not initialized");
        if (State != TaskState.Created) throw new InvalidOperationException("Task is already running");
        State = TaskState.Running;
        StateMachine.MoveNext();
    }

    internal void SetResult(TResult result)
    {
        if (State != TaskState.Running) throw new InvalidOperationException($"Task is not running: {State}");
        Result = result;
        State = TaskState.CompletedSuccessfully;
        StateMachine = null;
        RunCallOnCompleted();
    }

    internal void SetException(Exception exception)
    {
        if (exception == RustCancellationException.Instance)
        {
            if (State != TaskState.Cancelled) throw new InvalidOperationException("Task is not cancelled");
            StateMachine = null;
            return;
        }
        if (exception is ReturnException<TResult> e)
        {
            SetResult(e.Value);
            return;
        }
        if (State != TaskState.Running) throw new InvalidOperationException($"Task is not running: {State}");
        Exception = ExceptionDispatchInfo.Capture(exception);
        State = TaskState.Failed;
        StateMachine = null;
        RunCallOnCompleted();
    }

    internal void AwaitOnCompleted<TAwaiter, TStateMachine>(
        ref TAwaiter awaiter, ref TStateMachine stateMachine)
        where TAwaiter : INotifyCompletion, IRustAwaiter
        where TStateMachine : IAsyncStateMachine
    {
        _awaitingTaskAwaiter = awaiter;
        awaiter.OnCompleted(stateMachine.MoveNext);
    }

    private void RunCallOnCompleted()
    {
        _onCompleted?.Invoke();
    }
}

// This interface must not be implemented.
public interface INeverImplemented
{
}

public interface IRustAwaiter : INotifyCompletion
{
    public void Cancel();
}

public struct RustAwaiter<T> : IRustAwaiter
{
    private RustTask<T> _rustTask;
    public RustAwaiter(RustTask<T> rustTask) => _rustTask = rustTask;
    public bool IsCompleted => _rustTask.IsCompleted;
    public void OnCompleted(Action completion) => _rustTask.OnCompleted(completion);
    public T GetResult() => _rustTask.GetResult();
    public void Cancel() => _rustTask.Cancel();
}

public struct RustTaskBuilder<T>
{
    public static RustTaskBuilder<T> Create() => new RustTaskBuilder<T>();

    public void Start<TStateMachine>(ref TStateMachine stateMachine)
        where TStateMachine : IAsyncStateMachine
    {
        // We assign task before boxing stateMachine to let both on-stack and boxed TStateMachine have Task
        Task = new RustTask<T>();
        Task.StateMachine = stateMachine;
    }

    public void SetStateMachine(IAsyncStateMachine stateMachine) => 
        throw new NotSupportedException("SetStateMachine is legacy function that are not used");

    public void SetException(Exception exception) => Task.SetException(exception);
    public void SetResult(T result) => Task.SetResult(result);

    public void AwaitOnCompleted<TAwaiter, TStateMachine>(
        ref TAwaiter awaiter, ref TStateMachine stateMachine)
        where TAwaiter : INotifyCompletion, IRustAwaiter
        where TStateMachine : IAsyncStateMachine =>
        Task.AwaitOnCompleted(ref awaiter, ref stateMachine);

    public void AwaitUnsafeOnCompleted<TAwaiter, TStateMachine>(
        ref TAwaiter awaiter, ref TStateMachine stateMachine)
        where TAwaiter : struct, ICriticalNotifyCompletion, INeverImplemented
        where TStateMachine : IAsyncStateMachine =>
        throw new NotSupportedException();

    public RustTask<T> Task { get; private set; }
}

public class RustCancellationException : Exception
{
    public static RustCancellationException Instance { get; } = new RustCancellationException();
    private RustCancellationException() { }
}

public static class RustTask
{
    public static RustTask<T> New<T>(Func<RustTask<T>> closure) => closure();

    public static async RustTask<T> FromTaskWithCancellationToken<T>(Func<CancellationToken, Task<T>> action)
    {
        var cts = new CancellationTokenSource();
        var task = action(cts.Token);
        var awaiter = task.GetAwaiter();
        if (awaiter.IsCompleted)
        {
            return awaiter.GetResult();
        }
        else
        {
            return await new TaskWrapper<T>(awaiter, cts);
        }
    }

    private class TaskWrapper<TResult> : IRustAwaiter
    {
        private TaskAwaiter<TResult> _underlying;
        private Action? _continuation;
        private readonly CancellationTokenSource _cts;

        public TaskWrapper(TaskAwaiter<TResult> underlying, CancellationTokenSource cts)
        {
            _underlying = underlying;
            _cts = cts;
            _underlying.OnCompleted(OnCompleted);
        }

        public TaskWrapper<TResult> GetAwaiter() => this;

        public bool IsCompleted => _underlying.IsCompleted || _cts.IsCancellationRequested;

        public TResult GetResult()
        {
            if (_cts.IsCancellationRequested) throw RustCancellationException.Instance;
            return _underlying.GetResult();
        }

        private void OnCompleted()
        {
            var continuation = _continuation;
            _continuation = null;
            continuation?.Invoke();
        }

        public void OnCompleted(Action continuation) => _continuation = continuation;

        public void Cancel()
        {
            _cts.Cancel();
            OnCompleted();
        }
    }
}
