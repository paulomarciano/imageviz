/**
 * Jotai atoms for SSE (Server-Sent Events) connection state.
 *
 * Tracks connection status, recent events (for debugging), and a counter
 * of new files received while the user is scrolled down.
 */

import { atom } from 'jotai';
import type { SseEvent } from '../types/api';

/** Connection status of the SSE stream. */
export type SseConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

/** The current connection status for the SSE stream. */
export const sseStatusAtom = atom<SseConnectionStatus>('disconnected');

/** Recent SSE events (keeps last 50 for debugging). */
export const recentSseEventsAtom = atom<SseEvent[]>([]);

/** Counter of new files received while the user is scrolled down. */
export const newFileCountAtom = atom<number>(0);
