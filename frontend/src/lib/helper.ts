import type { Treebranch } from "./state";

export function findbranchwithid(id: string, tree: Treebranch[] | undefined): Treebranch | undefined {
    if (!tree) return;

    for (let i = 0; i < tree.length; i++) {
        if (tree[i].id === id) {
            return tree[i];
        }
        if (tree[i].children) {
            const result = findbranchwithid(id, tree[i].children);
            if (result) {
                return result;
            }
        }
    }
}