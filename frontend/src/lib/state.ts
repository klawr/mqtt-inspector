
type Message = {
    timestamp: string,
    delta_t?: number,
    text: string,
}

export class AppState {
    selectedBroker: string = "";
    selectedTopic: Treebranch | null = null;
    brokerRepository: BrokerRepository = {};
    pipelines: SavedPipeline[] = [];
    commands: Command[] = [];
}

export type Treebranch = {
    id: string,
    text: string,
    children?: Treebranch[],
    original_text: string,
    number_of_messages: number,
    messages: Message[]
};

type PipelineEntry = {
    topic: string
}
export type SavedPipeline = {
    id: number;
    text: string;
    pipeline: PipelineEntry[]
}

export type Command = {
    id: string;
    text: string;
    topic: string;
    payload: string
}

export type BrokerRepositoryEntry = {
    topics: Treebranch[];
    selectedTopic: Treebranch | null;
    pipeline: { topic: string; timestamp?: string; delta_t?: number }[];

}

export type BrokerRepository = {
    [key: string]: BrokerRepositoryEntry;
}
