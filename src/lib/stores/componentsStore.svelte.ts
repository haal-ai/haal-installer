export interface ComponentInfo {
  id: string;
  name: string;
  description: string;
  version: string;
  componentType: string;
  author?: string;
  compatibleTools: string[];
  dependencies: string[];
  pinned: boolean;
  deprecated: boolean;
  fileSize: number;
  repository: string;
}

function createComponentsStore() {
  let available = $state<ComponentInfo[]>([]);
  let selected = $state<Set<string>>(new Set());
  let loading = $state(false);
  let searchQuery = $state("");
  let filterType = $state<string>("all");

  return {
    get available() {
      return available;
    },
    get selected() {
      return selected;
    },
    get loading() {
      return loading;
    },
    get searchQuery() {
      return searchQuery;
    },
    get filterType() {
      return filterType;
    },
    get selectedCount() {
      return selected.size;
    },

    setAvailable(components: ComponentInfo[]) {
      available = components;
    },

    setLoading(value: boolean) {
      loading = value;
    },

    setSearchQuery(query: string) {
      searchQuery = query;
    },

    setFilterType(type: string) {
      filterType = type;
    },

    toggleSelection(id: string) {
      const next = new Set(selected);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      selected = next;
    },

    selectAll() {
      selected = new Set(available.map((c) => c.id));
    },

    deselectAll() {
      selected = new Set();
    },

    isSelected(id: string) {
      return selected.has(id);
    },

    reset() {
      available = [];
      selected = new Set();
      loading = false;
      searchQuery = "";
      filterType = "all";
    },
  };
}

export const componentsStore = createComponentsStore();
