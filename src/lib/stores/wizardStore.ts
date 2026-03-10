export type WizardStep = "connect" | "choose" | "preview" | "execute" | "done";

export interface SelectedComponent {
  id: string;
  name: string;
  targetTools: string[];
}

function createWizardStore() {
  let currentStep = $state<WizardStep>("connect");
  let selectedComponents = $state<SelectedComponent[]>([]);
  let destinations = $state<Record<string, string>>({});
  let isExecuting = $state(false);

  const steps: WizardStep[] = [
    "connect",
    "choose",
    "preview",
    "execute",
    "done",
  ];

  return {
    get currentStep() {
      return currentStep;
    },
    get selectedComponents() {
      return selectedComponents;
    },
    get destinations() {
      return destinations;
    },
    get isExecuting() {
      return isExecuting;
    },
    get currentStepIndex() {
      return steps.indexOf(currentStep);
    },
    get canGoBack() {
      return steps.indexOf(currentStep) > 0 && !isExecuting;
    },
    get canGoForward() {
      return steps.indexOf(currentStep) < steps.length - 1 && !isExecuting;
    },

    setStep(step: WizardStep) {
      currentStep = step;
    },

    nextStep() {
      const idx = steps.indexOf(currentStep);
      if (idx < steps.length - 1) {
        currentStep = steps[idx + 1];
      }
    },

    prevStep() {
      if (!isExecuting) {
        const idx = steps.indexOf(currentStep);
        if (idx > 0) {
          currentStep = steps[idx - 1];
        }
      }
    },

    setSelectedComponents(components: SelectedComponent[]) {
      selectedComponents = components;
    },

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

    setExecuting(value: boolean) {
      isExecuting = value;
    },

    reset() {
      currentStep = "connect";
      selectedComponents = [];
      destinations = {};
      isExecuting = false;
    },
  };
}

export const wizardStore = createWizardStore();
