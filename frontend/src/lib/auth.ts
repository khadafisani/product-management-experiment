import { writable } from 'svelte/store';
export type User = { id: number; username: string; role: string; permissions: string[] };
export const user = writable<User | null>(null);

export const workspaceConnection = writable(true);
export const workspaceLowStock = writable(0);
