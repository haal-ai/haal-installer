export interface ProgressInfo {
  step: string;
  percentage: number;
  currentFile: string;
  elapsedMs: number;
  estimatedRemainingMs: number;
  componentsSucceeded: string[];
  componentsFailed: string[];
}

function createProgressStore() {
  let active = $state(false);
  let progress = $state<ProgressInfo>({
    step: "",
    percentage: 0,
    currentFile: "",
    elapsedMs: 0,
    estimatedRemainingMs: 0,
    componentsSucceeded: [],
    componentsFailed: [],
  });
  let rollbackPerformed = $state(false);

  return {
    get active() {
      return active;
    },
    get progress() {
      return progress;
    },
    get rollbackPerformed() {
      return rollbackPerformed;
    },

    start() {
      active = true;
      rollbackPerformed = false;
      progress = {
        step: "",
        percentage: 0,
        currentFile: "",
        elapsedMs: 0,
        estimatedRemainingMs: 0,
        componentsSucceeded: [],
        componentsFailed: [],
      };
    },

    update(info: Partial<ProgressInfo>) {
      progress = { ...progress, ...info };
    },

    complete() {
      active = false;
      progress = { ...progress, percentage: 100 };
    },

    setRollbackPerformed(value: boolean) {
      rollbackPerformed = value;
    },

    reset() {
      active = false;
      rollbackPerformed = false;
      progress = {
        step: "",
        percentage: 0,
        currentFile: "",
        elapsedMs: 0,
        estimatedRemainingMs: 0,
        componentsSucceeded: [],
        componentsFailed: [],
      };
    },
  };
}

export const progressStore = createProgressStore();
