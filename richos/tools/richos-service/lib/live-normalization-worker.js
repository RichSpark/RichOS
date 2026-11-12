import { parentPort, workerData } from 'node:worker_threads';
import { prepareLiveNormalization, recorderIsRunning } from './live-transcription.js';

const { dir, recorderPid } = workerData;
const controller = new AbortController();
parentPort.once('message', () => controller.abort());
try {
  await prepareLiveNormalization(dir, () => recorderIsRunning(recorderPid), controller.signal);
} catch (error) {
  if (!controller.signal.aborted) throw error;
} finally {
  parentPort.close();
}
