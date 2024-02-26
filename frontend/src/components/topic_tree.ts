
type message = {
    timestamp: string,
    delta_t?: number,
    text: string,
}

export type treebranch = {
    id: string,
    text: string,
    children?: treebranch[],
    original_text: string,
    number_of_messages: number,
    messages: message[]
};

function addToTopicBranch(
    topicsplit: string[],
    index: number,
    topicbranch: treebranch[] | undefined,
    payload: string,
    timestamp: string) {
    const key = topicsplit[index];
    const found = topicbranch?.find((element) => element.original_text === key);

    if (index === topicsplit.length) {
        return;
    }

    if (found) {
        found.children = found.children || [];
        found.number_of_messages += 1;
        found.text = `${found.original_text} (${found.number_of_messages} messages)`;
        addToTopicBranch(topicsplit, index + 1, found.children, payload, timestamp);
    }
    else {
        const newtreebranch = {
            id: topicsplit.slice(0, index + 1).join("/"),
            text: key + ' (1 message)',
            children: [],
            original_text: key,
            number_of_messages: 1,
            messages: [{
                timestamp, text: payload
            }]
        }
        topicbranch?.push(newtreebranch);
        addToTopicBranch(topicsplit, index + 1, newtreebranch.children, payload, timestamp);
    }

    topicbranch?.forEach((element) => {
        if (element.children?.length === 0) {
            element.children = undefined;
        }
    });

    if (index === topicsplit.length - 1) {
        const new_entry = { timestamp: timestamp, text: payload, delta_t: 0 }
        if (found?.messages.length) {
            new_entry.delta_t = new Date(timestamp).getTime() - new Date(found.messages[0].timestamp).getTime();
        }
        found?.messages.unshift(new_entry);
    }

    return topicbranch;
}

export function addToTopicTree(
    topic: string,
    topictree: treebranch[],
    payload: string,
    timestamp: string): treebranch[] {
    const branch = topic.split('/');

    return addToTopicBranch(branch, 0, topictree, payload, timestamp) || [];
}

export function findbranchwithid(id: string, tree: treebranch[] | undefined): treebranch | undefined {
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