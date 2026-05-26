import { inferAsyncReturnType } from '@trpc/server'
import { EventEmitter } from 'events';

export const createContext = async () => {
  const ee = new EventEmitter();
  const ctx = { ee }
  return ctx;
}

export type Context = inferAsyncReturnType<typeof createContext>;
