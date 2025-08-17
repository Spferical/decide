import { Component, createRef, Fragment, render, VNode } from 'preact';
import { useState } from 'preact/hooks';
import { route, Router } from 'preact-router';
import Cookies from 'js-cookie';
import { v4 as uuidv4 } from 'uuid';

function Index() {
    function rps() {
        const room = uuidv4().substring(0, 5);
        route(`/rps/${room}`);
    }
    function vote() {
        route("/vote/");
    }
    document.title = "Decide.pfe.io";
    return (
        <main>
            <h1>Decide.pfe.io</h1>
            <p>Welcome to Decide, the quickest way to run a fair ranked vote for a small group!</p>
            <p><button onClick={vote}>🗳️ Start a Vote</button></p>
            <p><button onClick={rps}>🪨📄✂️ Play Rock Paper Scissors</button></p>
            <h2>About</h2>
            <p>Decide.pfe.io is a simple website for running a short ranked vote for a small group.</p>
            <ul>
                <li>No login.</li>
                <li>Candidates are shuffled for each voter to help avoid bias.</li>
                <li>All data is wiped after 24 hours.</li>
                <li>Rock-paper-scissors is here, too, for tiebreaking.</li>
            </ul>
            <h2>How are votes tallied?</h2>
            <p>Decide.pfe.io uses a <a href="https://en.wikipedia.org/wiki/Condorcet_method">Condorcet method</a>, specifically <a href="https://en.wikipedia.org/wiki/Ranked_pairs">ranked pairs</a>, to calculate the winner of each election. Condorcet methods are the only voting methods that guarantee the majority wins.</p>
            <h2>More Details</h2>
            <p>Decide.pfe.io is free and open source. <a href="https://github.com/Spferical/decide">The source code is available here</a>.</p>
            <p>Please contact me at <a href="mailto:matthew@pfe.io">matthew@pfe.io</a> with any questions or feedback!</p>
        </main>
    )
}

function get_vote_uuid() {
    let cookie = Cookies.get("VOTE_ID");
    if (cookie) {
        return cookie;
    }
    let uuid = uuidv4();
    Cookies.set("VOTE_ID", uuid, { sameSite: "strict", secure: true });
    return uuid;
}

function make_websocket(path: string) {
    const ws_protocol = (window.location.protocol === "https:") ? "wss://" : "ws://";
    const uri = ws_protocol + window.location.host + path;
    return new WebSocket(uri);
}

type RpsProps = {
    room: string
}

type PlayerView = {
    choice: string | null,
    opponent_chosen: boolean
    outcome_history: string[]
    wins: number
    draws: number
    losses: number
}

type SpectatorView = {
    player_wins: number[]
    player_chosen: boolean[]
    draws: number
}

type RoomView = {
    num_players: number
    num_spectators: number
    history: string[][]
    player_view: PlayerView | null
    spectator_view: SpectatorView | null
}

type RpsState = {
    room: string,
    status: string,
    room_state: RoomView | null
}

function CopyLink() {
    const initial_text = "<-- Click to copy!";
    const [infoText, setText] = useState(initial_text);
    const doCopy = () => {
        navigator.clipboard.writeText(window.location.href);
        setText("<-- Copied!");
        setTimeout(() => setText(initial_text), 1000);
    };
    return <Fragment>
        <span
            class="copyurl"
            onClick={e => doCopy()}
            onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { doCopy() } }}
            role="button"
            tabIndex={0}
        >
            {window.location.href}
        </span> {infoText}
    </Fragment>;
}

class Rps extends Component<RpsProps, RpsState> {
    state = { room: null, status: "connecting", room_state: null };
    ws = null;

    componentDidMount() {
        document.title = "Rock Paper Scissors"
    }

    componentWillUnmount() {
        try {
            this.ws.close();
        } catch (_) { }
    }

    render(props: RpsProps, state: RpsState) {
        if (state.room !== props.room) {
            state.room = props.room;
            this.ws = make_websocket(`/api/rps/${state.room}`);
            this.ws.onclose = evt => {
                console.log("Websocket disconnected!");
                console.log(evt);
                this.setState({ status: "disconnected" })
            };
            this.ws.onmessage = msg => this.setState(JSON.parse(msg.data));
        }
        if (state.status === "connecting") {
            return <footer role="status">Connecting...</footer>;
        } else if (state.status === "disconnected") {
            return <footer role="status">Disconnected! Try refreshing.</footer>;
        }

        console.assert(state.room_state != null);

        let player_view = state.room_state.player_view;
        let spectator_view = state.room_state.spectator_view;
        const is_player = !!player_view;
        const get_onclick = (choice: string) => () => {
            this.ws.send(JSON.stringify({ choice }))
        };

        let history = state.room_state.history;
        let history_component: VNode;
        if (history.length > 0) {
            let items = [];
            for (let i = 0; i < history.length; i++) {
                let item = `${history[i][0]} vs ${history[i][1]}`;
                if (is_player) {
                    item = `${player_view.outcome_history[i]}: ${item}`;
                }
                items.push(<li key={i}>{item}</li>)
            }
            history_component = (
                <div>
                    <h3>History:</h3>
                    <ol>{items}</ol>
                </div>
            );
        }

        return (
            <Fragment>
                <main>
                    {is_player && <div>
                        {(state.room_state.num_players < 2) && <p class="notice" role="status">
                            Send this URL to your opponent to connect.<br /> <CopyLink />
                        </p>}
                        <p>
                            <button onClick={get_onclick("rock")}>rock</button>
                            {" "}
                            <button onClick={get_onclick("paper")}>paper</button>
                            {" "}
                            <button onClick={get_onclick("scissors")}>scissors</button>
                        </p>
                        {player_view.choice && <p role="status">You have selected: {player_view.choice}.</p>}
                        {state.room_state.num_players >= 2 &&
                            <p role="status">{player_view.opponent_chosen ? "Opponent has selected a choice." : "Waiting for opponent to select..."}</p>}
                        {!!(player_view.wins || player_view.losses || player_view.draws) &&
                            <div role="status">Wins: {player_view.wins} Losses: {player_view.losses} Draws: {player_view.draws}</div>}
                    </div>}
                    {!!spectator_view && !!(spectator_view.player_wins || spectator_view.draws) &&
                        <div role="status"> Wins: {spectator_view.player_wins.join(" vs ")} Draws: {spectator_view.draws}</div>
                    }
                    {history_component}
                </main>
                <footer>
                    <div role="status">There are {state.room_state.num_players} players and {state.room_state.num_spectators} spectators.</div>
                    <div role="status">{is_player ? "You are a player!" : "You are a spectator!"}</div>
                </footer>
            </Fragment>
        );
    }
}

function describe_vote(choices: string[], vote: UserVote) {
    let s = `${vote.name}: `;
    for (let j = 0; j < vote.selections.length; j++) {
        let vi = vote.selections[j];
        if (j !== 0) {
            if (vote.selections[j].rank !== vote.selections[j - 1].rank) {
                s += " > ";
            } else {
                s += " = ";
            }
        }
        s += choices[vi.candidate];
    }
    return s;
}

function shuffle_array(arr: any[]) {
    for (let i = arr.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [arr[i], arr[j]] = [arr[j], arr[i]];
    }
}

type VoteItem = {
    candidate: number,
    rank: number,
}

type ChoicesProps = {
    choices: string[],
    initial_ranks: VoteItem[],
};

type ChoicesState = {
    // Sorted from rank 1 (top) to bottom. Stores indices into the candidate array.
    ranks: number[][],
    // Choice currently being dragged on a mobile browser. Present only during dragging.
    draggedChoice: number | null,
    // Integer row index the choice is currently dragged over.
    dragTarget: number | null,
    // Location of the last drag event.
    dragPos: { x: number, y: number } | null,
};

class Choices extends Component<ChoicesProps, ChoicesState> {
    constructor(props: ChoicesProps) {
        super();
        this.state = {
            ranks: [Array(props.choices.length).fill(null).map((_, i) => i)],
            draggedChoice: null,
            dragTarget: null,
            dragPos: null,
        };
        this.setSelections(props.initial_ranks);
    }

    onChoiceClick(i: number, e: MouseEvent) {
        e.stopPropagation();
        // Focus the clicked element instead of setting state
        const element = e.currentTarget as HTMLElement;
        element.focus();
    }

    onRowClick(targetRankIndex: number) {
        // Get the currently focused choice element
        const focusedElement = document.activeElement as HTMLElement;
        if (focusedElement && focusedElement.hasAttribute('data-choice')) {
            const choiceIndex = parseInt(focusedElement.getAttribute('data-choice')!);
            this.moveChoiceToRank(choiceIndex, targetRankIndex);
        }
    }

    onContextMenu(choiceIndex: number, currentRankIndex: number, e: MouseEvent) {
        e.preventDefault();
        this.splitChoiceToNewRank(choiceIndex, currentRankIndex);
    }

    onChoiceTouchStart(choiceIndex: number, e: TouchEvent) {
        e.preventDefault();
        const touch = e.touches[0];
        this.setState({
            draggedChoice: choiceIndex,
            dragPos: {
                x: touch.pageX,
                y: touch.pageY,
            }
        });
        const element = e.currentTarget as HTMLElement;
        element.focus();
    }

    onChoiceTouchMove(choiceIndex: number, e: TouchEvent) {
        e.preventDefault();
        if (this.state.draggedChoice === choiceIndex) {
            const touch = e.touches[0];
            this.setState({
                dragPos: {
                    x: touch.pageX,
                    y: touch.pageY,
                }
            });
            const element = document.elementFromPoint(touch.clientX, touch.clientY);
            const row = element?.closest('tr');
            if (row && row.dataset.rank !== undefined) {
                const rankIndex = Number(row.dataset.rank);
                if (rankIndex !== this.state.dragTarget) {
                    this.setState({ dragTarget: rankIndex });
                }
            }
        }
    }

    onChoiceTouchEnd(choiceIndex: number, e: TouchEvent) {
        e.preventDefault();
        if (this.state.draggedChoice === choiceIndex) {
            this.finishDrag();
        }
    }

    onRowDragOver(e: DragEvent) {
        e.preventDefault();
    }

    onRowDrop(e: DragEvent, targetRankIndex: number) {
        e.preventDefault();
        const focusedElement = document.activeElement as HTMLElement;
        if (focusedElement && focusedElement.hasAttribute('data-choice')) {
            const choiceIndex = parseInt(focusedElement.getAttribute('data-choice')!);
            this.moveChoiceToRank(choiceIndex, targetRankIndex);
        }
    }

    onRowMouseDown(e: MouseEvent) {
        console.log(e);
        if (e.target instanceof HTMLElement) {
            if (!e.target.dataset.choice) {
                // Prevent focusing <body> when clicking a row.
                // This may keep a choice button focused for the click handler.
                e.preventDefault();
            }
        }
    }

    onKeyDown(choiceIndex: number, e: KeyboardEvent) {
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            this.onChoiceClick(choiceIndex, e as any);
        } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            e.preventDefault();
            const currentRank = this.getChoiceRank(choiceIndex);
            const targetRank = e.key === 'ArrowUp' ? currentRank - 1 : currentRank + 1;
            this.moveChoiceToRank(choiceIndex, targetRank, true);
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
            e.preventDefault();
            const currentRank = this.getChoiceRank(choiceIndex);
            const choicesInRank = this.state.ranks[currentRank];
            const currentIndex = choicesInRank.indexOf(choiceIndex);
            if (e.key === 'ArrowLeft' && currentIndex > 0) {
                const prevChoice = choicesInRank[currentIndex - 1];
                const prevElement = document.querySelector(`[data-choice="${prevChoice}"]`) as HTMLElement;
                if (prevElement) prevElement.focus();
            } else if (e.key === 'ArrowRight' && currentIndex < choicesInRank.length - 1) {
                const nextChoice = choicesInRank[currentIndex + 1];
                const nextElement = document.querySelector(`[data-choice="${nextChoice}"]`) as HTMLElement;
                if (nextElement) nextElement.focus();
            }
        }
    }

    onMoveUp(choiceIndex: number, e: MouseEvent) {
        e.stopPropagation();
        const currentRank = this.getChoiceRank(choiceIndex);
        this.moveChoiceToRank(choiceIndex, currentRank - 1);
    }

    onMoveDown(choiceIndex: number, e: MouseEvent) {
        e.stopPropagation();
        const currentRank = this.getChoiceRank(choiceIndex);
        this.moveChoiceToRank(choiceIndex, currentRank + 1);
    }

    getChoiceRank(choiceIndex: number): number {
        for (let r = 0; r < this.state.ranks.length; r++) {
            if (this.state.ranks[r].includes(choiceIndex)) {
                return r;
            }
        }
        return -1;
    }

    finishDrag() {
        if (this.state.draggedChoice !== null && this.state.dragTarget !== null) {
            this.moveChoiceToRank(this.state.draggedChoice, this.state.dragTarget);
        }
        this.setState({
            draggedChoice: null,
            dragTarget: null,
            dragPos: null,
        });
    }

    moveChoiceToRank(choiceIndex: number, targetRankIndex: number, keepFocused = false) {
        let currentRankIndex = -1, currentPos = -1;
        for (let r = 0; r < this.state.ranks.length; r++) {
            for (let p = 0; p < this.state.ranks[r].length; p++) {
                if (this.state.ranks[r][p] === choiceIndex) {
                    currentRankIndex = r;
                    currentPos = p;
                    break;
                }
            }
            if (currentRankIndex !== -1) break;
        }
        if (currentRankIndex === targetRankIndex) return;

        let ranks = this.state.ranks.map(rank => [...rank]);
        ranks[currentRankIndex].splice(currentPos, 1);
        while (ranks.length <= targetRankIndex) ranks.push([]);
        if (targetRankIndex === -1) {
            ranks.unshift([choiceIndex])
        } else {
            ranks[targetRankIndex].push(choiceIndex);
        }
        this.setState({ ranks: ranks.filter(rank => rank.length > 0) }, () => {
            if (keepFocused) {
                // Use requestAnimationFrame to ensure DOM is updated
                requestAnimationFrame(() => {
                    const movedElement = document.querySelector(`[data-choice="${choiceIndex}"]`) as HTMLElement;
                    if (movedElement) {
                        movedElement.focus();
                    }
                });
            }
        });
    }

    splitChoiceToNewRank(choiceIndex: number, currentRankIndex: number) {
        let ranks = this.state.ranks.map(rank => [...rank]);
        ranks[currentRankIndex] = ranks[currentRankIndex].filter(choice => choice !== choiceIndex);
        ranks.splice(currentRankIndex + 1, 0, [choiceIndex]);
        this.setState({ ranks: ranks.filter(rank => rank.length > 0) });
    }

    render(props: ChoicesProps, state: ChoicesState) {
        let tableRows = [];
        for (let rankIndex = 0; rankIndex < state.ranks.length; rankIndex++) {
            const choices = state.ranks[rankIndex].map((choiceIndex) => (
                <span key={choiceIndex} style={{ display: 'inline-flex', alignItems: 'center', gap: '2px' }}>
                    <span
                        data-choice={choiceIndex}
                        role="button"
                        tabIndex={0}
                        class="choice"
                        draggable={true}
                        onClick={(e: MouseEvent) => this.onChoiceClick(choiceIndex, e)}
                        onKeyDown={(e: KeyboardEvent) => this.onKeyDown(choiceIndex, e)}
                        onContextMenu={(e: MouseEvent) => {
                            e.preventDefault();
                            this.splitChoiceToNewRank(choiceIndex, rankIndex);
                        }}
                        onTouchStart={(e: TouchEvent) => this.onChoiceTouchStart(choiceIndex, e)}
                        onTouchMove={(e: TouchEvent) => this.onChoiceTouchMove(choiceIndex, e)}
                        onTouchEnd={(e: TouchEvent) => this.onChoiceTouchEnd(choiceIndex, e)}
                        style={{
                            cursor: 'grab',
                            userSelect: 'none',
                            touchAction: 'none'
                        }}
                    >
                        {props.choices[choiceIndex]}
                    </span>
                    <span style={{ display: 'flex', flexDirection: 'column', gap: '1px' }}>
                        <button
                            onClick={(e: MouseEvent) => this.onMoveUp(choiceIndex, e)}
                            title="Move up"
                            class="arrow-button"
                        >
                            ▲
                        </button>
                        <button
                            onClick={(e: MouseEvent) => this.onMoveDown(choiceIndex, e)}
                            title="Move down"
                            class="arrow-button"
                        >
                            ▼
                        </button>
                    </span>
                </span>
            ));

            tableRows.push(
                <tr
                    key={`rank-${rankIndex}`}
                    data-rank={rankIndex}
                    class={state.dragTarget === rankIndex ? "ballot-row drag-target" : "ballot-row"}
                    onMouseDown={this.onRowMouseDown}
                    onClick={(e) => this.onRowClick(rankIndex)}
                    onDragOver={(e: DragEvent) => this.onRowDragOver(e)}
                    onDrop={(e: DragEvent) => this.onRowDrop(e, rankIndex)}
                    onKeyDown={(e: KeyboardEvent) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            this.onRowClick(rankIndex);
                        }
                    }}
                >
                    <td class="ballot-rank-cell">Rank {rankIndex + 1}</td>
                    <td class="ballot-choices-cell">{choices}</td>
                </tr>
            );
        }

        tableRows.push(
            <tr
                key="empty-row"
                data-rank={state.ranks.length}
                class={state.dragTarget === state.ranks.length ? "ballot-row ballot-empty-row drag-target" : "ballot-row ballot-empty-row"}
                onMouseDown={this.onRowMouseDown}
                onClick={() => this.onRowClick(state.ranks.length)}
                onDragOver={(e: DragEvent) => this.onRowDragOver(e)}
                onDrop={(e: DragEvent) => this.onRowDrop(e, state.ranks.length)}
                onKeyDown={(e: KeyboardEvent) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        this.onRowClick(state.ranks.length);
                    }
                }}
            >
                <td class="ballot-rank-cell">Rank {state.ranks.length + 1}</td>
                <td class="ballot-empty-cell">Drop choices here to create a new rank</td>
            </tr>
        );

        return (
            <Fragment>
                <table class="ballot-table" role="grid" aria-label="Ballot">
                    <tbody>{tableRows}</tbody>
                </table>
                {state.dragPos && (
                    <div
                        class="drag-overlay"
                        style={{
                            left: state.dragPos.x,
                            top: state.dragPos.y,
                            pointerEvents: 'none'
                        }}
                        aria-hidden="true"
                    >
                        {props.choices[state.draggedChoice]}
                    </div>
                )}
            </Fragment>
        );
    }

    getSelections() {
        let items = [];
        for (let rankIndex = 0; rankIndex < this.state.ranks.length; rankIndex++) {
            for (let choiceIndex of this.state.ranks[rankIndex]) {
                items.push({ candidate: choiceIndex, rank: rankIndex });
            }
        }
        return items;
    }

    setSelections(items: VoteItem[]) {
        if (items.length === 0) return;
        let ranks: number[][] = [];
        for (let item of items) {
            while (ranks.length <= item.rank) ranks.push([]);
            ranks[item.rank].push(item.candidate);
        }
        this.setState({ ranks });
    }
}

type UserVote = {
    selections: VoteItem[],
    name: string,
}

type Tally = {
    totals: number[][],
    ranks: number[][],
}

type Results = {
    votes: UserVote[]
    tally: Tally,
}

function VoteResults({ choices, results }: { choices: string[], results: Results }) {
    const votes = results.votes.map((v, i) => <li key={i}>{describe_vote(choices, v)}</li>);
    votes.sort();
    const tchoices = choices.map((c, i) => <th key={i} scope="row">{c}</th>);
    const thead = <thead><tr><th key="head" scope="col" />{tchoices}</tr></thead>;
    const totals = results.tally.totals;
    const trows = totals.map((_, i) => {
        const tds = totals[i].map((val, j) => {
            const symbol = (i === j) ? "-" : (val > totals[j][i]) ? <mark>{val}</mark> : val.toString();
            return <td key={j}>{symbol}</td>
        });
        return <tr key={i}><th key="head" scope="row">{choices[i]}</th>{tds}</tr>
    });
    const ranks = results.tally.ranks.map(
        (rank, i) => <li key={i}>{rank.map(c => choices[c]).join(" AND ")}</li>
    );
    const winners = (results.tally.ranks[0] || []).map(i => choices[i]);
    let winner_desc = (winners.length > 1) ? "winners are" : "winner is";
    return <article>
        <header role="banner">
            <h2>The results are in! The {winner_desc}: <strong>{winners.join(" AND ")}</strong></h2>
        </header>
        <details>
            <summary>See detailed results</summary>
            <p>The votes are:</p>
            <ul>
                {votes}
            </ul>
            <table role="grid" aria-label="Vote comparison matrix">
                {thead}
                <tbody>{trows}</tbody>
            </table>
            <p>The full ranks are:</p>
            <ol>
                {ranks}
            </ol>
        </details>
    </article>
}

type VoteProps = {
    room: string
}

type VoteView = {
    choices: string[]
    your_vote: UserVote | null
    num_votes: number
    num_players: number
    results: Results | null
}

type VoteState = {
    room: string | null
    status: string
    voter_name: string
    vote: VoteView | null
}

class Vote extends Component<VoteProps, VoteState> {
    state = { room: null, status: "connecting", voter_name: "", vote: null };
    ws: WebSocket | null = null;
    choices_component = createRef();
    initial_vote: UserVote | null = null;

    componentDidMount() {
        document.title = "Vote";
    }

    componentWillUnmount() {
        try {
            this.ws.close();
        } catch (_) { }
    }

    render(props: VoteProps, state: VoteState) {
        if (!props.room) {
            return <Fragment>
                <main>
                    <h1>Start a Vote</h1>
                    <form action="/api/start_vote" method="post">
                        <p><label for="choices">Enter the choices up for vote, one per line:</label></p>
                        <p><textarea name="choices" id="choices" required aria-required="true" /></p>
                        <input type="submit" value="Start Vote" />
                    </form>
                </main>
            </Fragment>
        }

        if (state.room !== props.room) {
            // The client connected to a new room. Perform initial setup.
            state.room = props.room;
            this.ws = make_websocket(`/api/vote/${state.room}?id=${get_vote_uuid()}`);
            this.ws.onclose = evt => {
                console.log("Websocket disconnected!");
                console.log(evt);
                this.setState({ status: "disconnected" });
            };
            this.ws.onmessage = msg => {
                let new_state = JSON.parse(msg.data);
                this.setState(new_state)
            };
        }
        if (state.status === "connecting") {
            return <footer role="status">Connecting...</footer>
        } else if (state.status === "disconnected") {
            return <footer role="status">Disconnected! Try refreshing.</footer>
        } else if (state.status === "invalid_room") {
            route("/vote");
            return <footer role="status">Invalid room!</footer>
        }

        console.assert(state.vote != null);

        const on_input = (event: Event) => this.setState(
            { voter_name: (event.target as HTMLInputElement).value }
        );

        let submitted_section = null;
        if (state.vote.your_vote) {
            let description = describe_vote(state.vote.choices, state.vote.your_vote);
            submitted_section = <p role="status">You submitted: {description}</p>;
        }

        const submit = () => {
            const choices_component = this.choices_component.current;
            let items = choices_component.getSelections();
            this.ws.send(JSON.stringify({ vote: { name: this.state.voter_name, selections: items } }))
        };

        const tally = () => this.ws.send(JSON.stringify({ tally: null }));

        let results = null;
        if (state.vote.results) {
            results = <VoteResults choices={state.vote.choices} results={state.vote.results} />
        }

        const submit_text = (state.vote.your_vote) ? "Resubmit Your Vote" : "Submit Your Vote";

        if (!this.initial_vote) {
            let vote = state.vote.your_vote;
            if (vote) {
                this.initial_vote = vote;
                this.setState({ voter_name: this.initial_vote.name })
            } else {
                let initial_order = Array(state.vote.choices.length).fill(null).map((_, i) => i);
                shuffle_array(initial_order);
                let initial_selections =
                    initial_order.map(
                        (candidate_idx, i) => ({ candidate: candidate_idx, rank: i }),
                    );
                this.initial_vote = {
                    name: "???",
                    selections: initial_selections
                }
                this.setState({ voter_name: this.initial_vote.name })
            }
        }

        const ballot_section = (
            <Fragment>
                <p>Click or drag or use tab/arrow keys to edit your ballot. Rank 1 is best.</p>
                <div role="region" aria-label="Voting ballot">
                    <Choices ref={this.choices_component} choices={state.vote.choices} initial_ranks={this.initial_vote.selections} />
                </div>
                <p>
                    <label for="voter_name">Voter name (optional):</label>
                    <input id="voter_name" value={state.voter_name} onInput={on_input} />
                </p>
                <p><button onClick={submit}>{submit_text}</button></p>
            </Fragment>
        );

        return (
            <Fragment>
                <main>
                    {state.vote.num_players <= 1 && <p class="notice" role="status"> Send this URL to all voters:<br /><CopyLink /> </p>}
                    {!state.vote.results && ballot_section}
                    {submitted_section}
                    {!state.vote.results && <p><button onClick={tally}>End Voting and Show the Results</button></p>}
                    <p role="status">{state.vote.num_votes}/{state.vote.num_players} voters have submitted ballots.</p>
                    {results}
                </main>
                <footer>
                    <p>This election will be <strong>deleted</strong> after 24 hours of inactivity.</p>
                    <p><a href="/vote">Click here to create a new election.</a></p>
                </footer>
            </Fragment>
        );
    }
}

class ErrorBoundary extends Component {
    state = { error: null }

    componentDidCatch(error) {
        console.error(error)
        this.setState({ error })
    }

    render() {
        if (this.state.error) {
            return <Fragment>
                <main>
                    <h1>Error</h1>
                    <p role="alert">Oops! Something went wrong.</p>
                    <p><a href="javascript:void(0)" onClick={() => window.location.reload()}>Click here to refresh the page.</a></p>
                    <section>
                        <p>Error details:</p>
                        <details>
                            <summary>Extra details</summary>
                            <pre>{this.state.error.toString()}</pre>
                            <details>
                                <summary>Stack trace</summary>
                                <pre>{this.state.error.stack}</pre>
                            </details>
                        </details>
                    </section>
                </main>
            </Fragment>
        }
        return this.props.children
    }
}

export default function App() {
    return (
        <ErrorBoundary>
            <Router>
                {/* @ts-ignore */}
                <Index path="/" />
                {/* @ts-ignore */}
                <Rps path="/rps/:room?" />
                {/* @ts-ignore */}
                <Vote path="/vote/:room?" />
            </Router>
        </ErrorBoundary>
    );
}

render(<App />, document.body);
