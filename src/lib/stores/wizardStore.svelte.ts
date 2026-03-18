import type { ResolvedComponent } from "./componentsStore.svelte";

export type WizardStep = "connect" | "choose" | "preview" | "execute";
export type InstallScope = "home" | "repo" | "both";

export interface SelectedComponent {
  id: string;
  name: string;
  targetTools: string[];
}

export interface InstallRequest {
  components: ResolvedComponent[];
  scope: InstallScope;
  repoPaths: string[];
  selectedTools: string[];
  reinstallAll: boolean;
  cleanInstall: boolean;
}

function createWizardStore() {
  let currentStep = $state<WizardStep>("connect");
  let selectedComponents = $state<SelectedComponent[]>([]);
  let destinations = $state<Record<string, string>>({});
  let isExecuting = $state(false);
  let registryUrl = $state<string>("");
  let isConnected = $state(false);
  let reinstallAll = $state(false);
  let cleanInstall = $state(false);
  let installScope = $state<InstallScope>("home");
  let repoPaths = $state<string[]>([]);
  let installRequest = $state<InstallRequest | null>(null);

  const steps: WizardStep[] = [
    "connect",
    "choose",
    "preview",
    "execute",
  ];

  return {
    get currentStep() { return currentStep; },
    get selectedComponents() { return selectedComponents; },
    get destinations() { return destinations; },
    get isExecuting() { return isExecuting; },
    get registryUrl() { return registryUrl; },
    get isConnected() { return isConnected; },
    get reinstallAll() { return reinstallAll; },
    get cleanInstall() { return cleanInstall; },
    get installScope() { return installScope; },
    get repoPaths() { return repoPaths; },
    get installRequest() { return installRequest; },
    get currentStepIndex() { return steps.indexOf(currentStep); },
    get canGoBack() { return steps.indexOf(currentStep) > 0 && !isExecuting; },
    get canGoForward() {
      if (isExecuting) return false;
      if (currentStep === "connect") return isConnected;
      return steps.indexOf(currentStep) < steps.length - 1;
    },

    setStep(step: WizardStep) { currentStep = step; },

    nextStep() {
      const idx = steps.indexOf(currentStep);
      if (idx < steps.length - 1) currentStep = steps[idx + 1];
    },

    prevStep() {
      if (!isExecuting) {
        const idx = steps.indexOf(currentStep);
        if (idx > 0) currentStep = steps[idx - 1];
      }
    },

    setSelectedComponents(components: SelectedComponent[]) { selectedComponents = components; },

    addComponent(component: SelectedComponent) {
      if (!selectedComponents.find((c) => c.id === component.id)) {
        selectedComponents = [...selectedComponents, component];
      }
    },

    removeComponent(id: string) {
      selectedComponents = selectedComponents.filter((c) => c.id !== id);
    },

    setDestination(tool: string, path: string) {
      destinations = { ...destinations, [tool]: path };
    },

    setExecuting(value: boolean) { isExecuting = value; },
    setRegistryUrl(url: string) { registryUrl = url; },
    setConnected(value: boolean) { isConnected = value; },
    setReinstallAll(value: boolean) { reinstallAll = value; },
    setCleanInstall(value: boolean) { cleanInstall = value; },
    setInstallScope(value: InstallScope) { installScope = value; },
    setRepoPaths(value: string[]) { repoPaths = value; },
    toggleRepoPath(path: string) {
      if (repoPaths.includes(path)) {
        repoPaths = repoPaths.filter(p => p !== path);
      } else {
        repoPaths = [...repoPaths, path];
      }
    },
    setInstallRequest(req: InstallRequest) { installRequest = req; },

    reset() {
      currentStep = "connect";
      selectedComponents = [];
      destinations = {};
      isExecuting = false;
      registryUrl = "";
      reinstallAll = false;
      cleanInstall = false;
      isConnected = false;
      installScope = "home";
      repoPaths = [];
      installRequest = null;
    },
  };
}

export const wizardStore = createWizardStore();
