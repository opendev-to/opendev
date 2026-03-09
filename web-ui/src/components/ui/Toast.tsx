import { X } from 'lucide-react';
import { useToastStore, type ToastVariant } from '../../stores/toast';

const VARIANT_STYLES: Record<ToastVariant, string> = {
  info: 'bg-bg-200 border-border-300/30 text-text-100',
  success: 'bg-emerald-50 border-emerald-200 text-emerald-800',
  warning: 'bg-yellow-50 border-yellow-200 text-yellow-800',
  error: 'bg-red-50 border-red-200 text-red-800',
};

export function ToastContainer() {
  const toasts = useToastStore(state => state.toasts);
  const removeToast = useToastStore(state => state.removeToast);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed top-14 right-4 z-[10000] flex flex-col gap-2 max-w-sm">
      {toasts.map(toast => (
        <div
          key={toast.id}
          className={`flex items-center gap-2 px-3 py-2 rounded-lg border shadow-lg text-sm animate-slide-up ${VARIANT_STYLES[toast.variant]}`}
        >
          <span className="flex-1">{toast.message}</span>
          <button
            onClick={() => removeToast(toast.id)}
            className="flex-shrink-0 opacity-60 hover:opacity-100 transition-opacity"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      ))}
    </div>
  );
}
