using Microsoft.Win32.SafeHandles;

namespace Wickra;

/// <summary>
/// Owns an opaque native indicator handle and releases it via the indicator's
/// <c>_free</c> function. One generic handle type backs every indicator; the
/// correct free routine is captured at construction time.
/// </summary>
internal sealed class WickraHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    private readonly Action<nint> _free;

    internal WickraHandle(nint handle, Action<nint> free)
        : base(ownsHandle: true)
    {
        _free = free;
        SetHandle(handle);
    }

    /// <summary>
    /// The raw pointer for a per-tick <c>_update</c> call, which skips the
    /// AddRef/Release the marshaller wraps around every other call: at one call
    /// per input those two interlocked operations cost more than the update.
    /// </summary>
    /// <remarks>
    /// Still throws <see cref="ObjectDisposedException"/> once disposed. The
    /// caller must follow the native call with <c>GC.KeepAlive</c> on this
    /// handle, so the finalizer cannot release it mid-call. A <c>Dispose</c> on
    /// another thread while an update is in flight is not guarded; the
    /// indicator objects are not thread-safe to begin with.
    /// </remarks>
    internal nint Live
    {
        get
        {
            ObjectDisposedException.ThrowIf(IsClosed, this);
            return handle;
        }
    }

    protected override bool ReleaseHandle()
    {
        _free(handle);
        return true;
    }
}
