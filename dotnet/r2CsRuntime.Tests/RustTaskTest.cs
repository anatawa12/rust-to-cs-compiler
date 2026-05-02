namespace r2CsRuntime.Tests;

public class RustTaskTest
{
    [Fact]
    public void RunSuccessfully()
    {
        var ctx = new RustTaskTester();
        var task = ctx.TestTask();
        Assert.False(ctx.TestTaskStarted);
        var awaiter = task.GetAwaiter();
        Assert.True(ctx.TestTaskStarted);
        Assert.False(ctx.TestTaskCompleted);
        Assert.False(awaiter.IsCompleted);
        Assert.False(ctx.CancellationToken.IsCancellationRequested);
        ctx.InnerTaskSource.SetResult(1);
        Assert.True(ctx.TestTaskStarted);
        Assert.True(ctx.TestTaskCompleted);
        Assert.True(awaiter.IsCompleted);
        Assert.Equal(1, awaiter.GetResult());
    }

    [Fact]
    public void Cancel()
    {
        var ctx = new RustTaskTester();
        var task = ctx.TestTask();
        Assert.False(ctx.TestTaskStarted);
        var awaiter = task.GetAwaiter();
        Assert.True(ctx.TestTaskStarted);
        Assert.False(ctx.TestTaskCompleted);
        Assert.False(awaiter.IsCompleted);
        Assert.False(ctx.CancellationToken.IsCancellationRequested);

        task.Cancel();

        Assert.True(ctx.CancellationToken.IsCancellationRequested);
        Assert.True(awaiter.IsCompleted);
    }

    class RustTaskTester
    {
        public bool TestTaskStarted;
        public bool TestTaskCompleted;

        public TaskCompletionSource<int> InnerTaskSource = new();
        public CancellationToken CancellationToken = new();

        public async RustTask<int> TestTask()
        {
            TestTaskStarted = true;
            var result = await CsTaskWrapper();
            TestTaskCompleted = true;
            return result;
        }

        public async RustTask<int> CsTaskWrapper() =>
            await RustTask.FromTaskWithCancellationToken(async (token) =>
            {
                CancellationToken = token;
                return await InnerTaskSource.Task;
            });
    }
}