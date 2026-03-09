import { create } from 'zustand';

export type ToastVariant = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: string;
  message: string;
  variant: ToastVariant;
}

interface ToastState {
  toasts: Toast[];
  addToast: (message: string, variant?: ToastVariant, timeout?: number) => void;
  removeToast: (id: string) => void;
}

let nextId = 0;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],

  addToast: (message: string, variant: ToastVariant = 'info', timeout?: number) => {
    const id = String(++nextId);
    const toast: Toast = { id, message, variant };

    set(state => ({ toasts: [...state.toasts, toast] }));

    const autoTimeout = timeout ?? (variant === 'error' ? 6000 : 4000);
    setTimeout(() => {
      set(state => ({ toasts: state.toasts.filter(t => t.id !== id) }));
    }, autoTimeout);
  },

  removeToast: (id: string) => {
    set(state => ({ toasts: state.toasts.filter(t => t.id !== id) }));
  },
}));
