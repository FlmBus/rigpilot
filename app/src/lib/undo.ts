// Simple snapshot-based undo/redo over the project state.

const LIMIT = 200;

export class History {
  private undoStack: string[] = [];
  private redoStack: string[] = [];

  /** Call BEFORE applying a mutation. */
  push(current: unknown) {
    this.undoStack.push(JSON.stringify(current));
    if (this.undoStack.length > LIMIT) this.undoStack.shift();
    this.redoStack = [];
  }

  undo<T>(current: T): T | null {
    const prev = this.undoStack.pop();
    if (prev === undefined) return null;
    this.redoStack.push(JSON.stringify(current));
    return JSON.parse(prev);
  }

  redo<T>(current: T): T | null {
    const next = this.redoStack.pop();
    if (next === undefined) return null;
    this.undoStack.push(JSON.stringify(current));
    return JSON.parse(next);
  }

  clear() {
    this.undoStack = [];
    this.redoStack = [];
  }
}
