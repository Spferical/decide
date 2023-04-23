import { Component, createRef, Fragment, render, VNode } from 'preact';
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
            <h2> Decide.pfe.io </h2>
            <p> Welcome to Decide.pfe.io, the easiest way to run a quick and fair ranked vote for a small group! </p>
            <p><button href="javascript:void(0)" onClick={vote}>Start a Vote</button></p>
            <p><button href="javascript:void(0)" onClick={rps}>Play Rock Paper Scissors</button></p>
            <h4> About </h4>
            <p> Decide.pfe.io is a simple website for running a short ranked vote for a small group. Features include: </p>
            <ul>
                <li> No login required: Anyone with the URL can participate in a poll.</li>
                <li> Simple and intuitive interface: Just enter the options up for vote and send a link.</li>
                <li> Quick and easy to use: Finish voting at any time with just one click. </li>
                <li> Safe and secure: All data is wiped after 24 hours.</li>
                <li> Rock-paper-scissors: Settle ties with the classic rock-paper-scissors game.</li>
            </ul >
            <h4> How are votes tallied? </h4>
            <p> Decide.pfe.io uses a <a href="https://en.wikipedia.org/wiki/Condorcet_method">Condorcet method</a>, specifically <a href="https://en.wikipedia.org/wiki/Ranked_pairs">ranked pairs</a>, to calculate the winner of each election. Condorcet voting ensures that the winning option is the one that beats all other options in head-to-head matchups, making it fair and representative.</p>
            <p>See <a href="https://condorcet.ericgorr.net/">condorcet.ericgorr.net</a> for an excellent voting calculator.</p>
            <h4> More Details </h4>
            <p> Decide.pfe.io is free and open source. <a href="https://github.com/Spferical/decide">The source code is available here</a>.</p>
            <p>Please contact me at <a href="mailto: matthew@pfe.io">matthew@pfe.io</a> with any questions, comments, or concerns.</p>
        </main >
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
    const ws_protocol = (window.location.protocol == "https:") ? "wss://" : "ws://";
    const uri = ws_protocol + location.host + path;
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

class Rps extends Component<RpsProps, RpsState> {
    state = { room: null, status: "connecting", room_state: null };
    ws = null;

    componentDidMount() {
        document.title = "Rock Paper Scissors"
    }

    componentDidUnmount() {
        this.ws.close();
    }

    render(props: RpsProps, state: RpsState) {
        if (state.room != props.room) {
            state.room = props.room;
            this.ws = make_websocket(`/api/rps/${state.room}`);
            this.ws.onclose = evt => {
                console.log("Websocket disconnected!");
                console.log(evt);
                this.setState({ status: "disconnected" })
            };
            this.ws.onmessage = msg => this.setState(JSON.parse(msg.data));
        }
        if (state.status == "connecting") {
            return <footer>Connecting...</footer>;
        } else if (state.status == "disconnected") {
            return <footer>Disconnected! Try refreshing.</footer>;
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
                items.push(<li>{item}</li>)
            }
            history_component = (
                <div>
                    History:
                    <ol>{items}</ol>
                </div>
            );
        }

        return (
            <Fragment>
                <main>
                    {is_player && <div>
                        {(state.room_state.num_players < 2) && <p>Send this URL to your opponent to connect.</p>}
                        <p>
                            <button onClick={get_onclick("rock")}>rock</button>
                            {" "}
                            <button onClick={get_onclick("paper")}>paper</button>
                            {" "}
                            <button onClick={get_onclick("scissors")}>scissors</button>
                        </p>
                        {player_view.choice && <p>You have selected: {player_view.choice}.</p>}
                        {state.room_state.num_players >= 2 &&
                            <p>{player_view.opponent_chosen ? "Opponent has selected a choice." : "Waiting for opponent to select..."}</p>}
                        {!!(player_view.wins || player_view.losses || player_view.draws) &&
                            <div>Wins: {player_view.wins} Losses: {player_view.losses} Draws: {player_view.draws}</div>}
                    </div>}
                    {!!spectator_view && !!(spectator_view.player_wins || spectator_view.draws) &&
                        <div> Wins: {spectator_view.player_wins.join(" vs ")} Draws: {spectator_view.draws}</div>
                    }
                    {history_component}
                </main>
                <footer>
                    <div>There are {state.room_state.num_players} players and {state.room_state.num_spectators} spectators.</div>
                    <div>{is_player ? "You are a player!" : "You are a spectator!"}</div>
                </footer>
            </Fragment>
        );
    }
}

function describe_vote(choices: string[], vote: UserVote) {
    let s = `${vote.name}: `;
    for (let j = 0; j < vote.selections.length; j++) {
        let vi = vote.selections[j];
        if (j != 0) {
            if (vote.selections[j].rank != vote.selections[j - 1].rank) {
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
    order: number[],
    gt: boolean[],
    selected: number | null,
};

class Choices extends Component<ChoicesProps, ChoicesState> {
    constructor(props: ChoicesProps) {
        super();
        this.state = {
            // Mapping of sorted position -> choice index.
            order: Array(props.choices.length).fill(null).map((_, i) => i),
            // true if sorted choice i is > choice i+1, else false if equal.
            gt: Array(Math.max(0, props.choices.length - 1)).fill(true),
            selected: null,
        };
        this.set_selections(props.initial_ranks);
    }

    swap(i: number, j: number) {
        let order = this.state.order.slice();
        let tmp = order[j];
        order[j] = order[i];
        order[i] = tmp;
        return { order }
    }

    onChoiceClick(i: number) {
        if (this.state.selected == null) {
            this.setState({ selected: i });
        } else {
            this.setState({ ...this.swap(i, this.state.selected), selected: null });
        }
    }

    onRankClick(i: number) {
        let gt = this.state.gt.slice();
        gt[i] = !gt[i];
        this.setState({ gt });
    }

    onDragStart(i: number) {
        this.setState({ selected: i });
    }

    onDragEnter(i: number) {
        console.assert(this.state.selected != null);
        this.setState({ ...this.swap(i, this.state.selected), selected: i });
    }

    render(props: ChoicesProps, state: ChoicesState) {
        let choices = [];
        for (let i = 0; i < props.choices.length; i++) {
            const choice_str = props.choices[state.order[i]];
            const choice_onclick = () => this.onChoiceClick(i);
            const choice_class = this.state.selected == i ? "choice chosen" : "choice";
            const ondragstart = () => this.onDragStart(i);
            const ondragenter = () => this.onDragEnter(i);
            const choice = <span role="button" class={choice_class} draggable={true} onClick={choice_onclick} onDragStart={ondragstart} onDragEnter={ondragenter}>{choice_str}</span>;
            choices.push(choice);
            if (i + 1 != props.choices.length) {
                const rank_onclick = () => this.onRankClick(i);
                const symbol = this.state.gt[i] ? ">" : "=";
                let order_elem = <button class="ordering" onClick={rank_onclick}>{symbol}</button>;
                choices.push(" ");
                choices.push(order_elem);
                choices.push(" ");
            }
        }
        return <div>{choices}</div>;
    }

    get_selections() {
        let items = [];
        let rank = 0;
        for (let i = 0; i < this.state.order.length; i++) {
            let item = this.state.order[i];
            items.push({ candidate: item, rank });
            if (this.state.gt[i]) {
                rank++;
            }
        }
        return items;
    }

    set_selections(items: VoteItem[]) {
        if (items.length == 0) {
            return;
        }
        items.sort((a, b) => a.rank - b.rank);
        let choices = items.map(item => item.candidate);
        let gt = [];
        for (let i = 1; i < items.length; i++) {
            gt.push(items[i].rank > items[i - 1].rank);
        }
        this.setState({ order: choices, gt: gt })
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
    // NOTE: no JSX keys here, vote results are static at the moment.
    /* eslint-disable-next-line react/jsx-key */
    const votes = results.votes.map(v => <li>{describe_vote(choices, v)}</li>);
    /* eslint-disable-next-line react/jsx-key */
    const tchoices = choices.map(c => <th scope="row">{c}</th>);
    /* eslint-disable-next-line react/jsx-key */
    const thead = <thead><tr><th key="head" scope="col" />{tchoices}</tr></thead>;
    const totals = results.tally.totals;
    const trows = totals.map((_, i) => {
        const tds = totals[i].map((val, j) => {
            const symbol = (i == j) ? "-" : (val > totals[j][i]) ? <mark>{val}</mark> : val.toString();
            /* eslint-disable-next-line react/jsx-key */
            return <td>{symbol}</td>
        });
        /* eslint-disable-next-line react/jsx-key */
        return <tr><th key="head" scope="row">{choices[i]}</th>{tds}</tr>
    });
    const ranks = results.tally.ranks.map(
        /* eslint-disable-next-line react/jsx-key */
        rank => <li>{rank.map(c => choices[c]).join(" AND ")}</li>
    );
    const winners = (results.tally.ranks[0] || []).map(i => choices[i]);
    let winner_desc = (winners.length > 1) ? "winners are" : "winner is";
    return <article>
        <header>The results are in! The {winner_desc}: <strong>{winners.join(" AND ")}</strong></header>
        <details>
            <summary>See detailed results</summary>
            <p> The votes are: </p>
            <ul>
                {votes}
            </ul>
            <table role="grid">
                {thead}
                {trows}
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

    render(props: VoteProps, state: VoteState) {
        if (!props.room) {
            return <Fragment>
                <h2>Start a Vote</h2>
                <form action="/api/start_vote" method="post">
                    <p><label for="choices">Enter the choices up for vote, one per line:</label></p>
                    <p><textarea name="choices" /></p>
                    <input type="submit" value="Start Vote" />
                </form>
            </Fragment>
        }

        if (state.room != props.room) {
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
        if (state.status == "connecting") {
            return <footer>Connecting...</footer>
        } else if (state.status == "disconnected") {
            return <footer>Disconnected! Try refreshing.</footer>
        } else if (state.status == "invalid_room") {
            route("/vote");
            return <footer>Invalid room!</footer>
        }

        console.assert(state.vote != null);

        const on_input = (event: Event) => this.setState(
            { voter_name: (event.target as HTMLInputElement).value }
        );

        let submitted_section = null;
        if (state.vote.your_vote) {
            let description = describe_vote(state.vote.choices, state.vote.your_vote);
            submitted_section = <p>You submitted: {description}</p>;
        }

        const submit = () => {
            const choices_component = this.choices_component.current;
            let items = choices_component.get_selections();
            this.ws.send(JSON.stringify({ vote: { name: this.state.voter_name, selections: items } }))
        };

        const tally = () => this.ws.send(JSON.stringify({ tally: null }));

        let results = null;
        if (state.vote.results) {
            results = <VoteResults choices={state.vote.choices} results={state.vote.results} />
        }

        const submit_text = (state.vote.your_vote) ? "Resubmit Your Vote" : "Submit Your Vote";

        // initial_vote is false until the first render with server-provided room state.
        if (!this.initial_vote) {
            // If this is a new tab from an existing user, adjust the UI
            // to match the last submitted vote.
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
                <p>Click or drag to edit your ballot.</p>
                <Choices ref={this.choices_component} choices={state.vote.choices} initial_ranks={this.initial_vote.selections} />
                <p>
                    <label for="voter_name">Voter name (optional):</label>
                    <input value={state.voter_name} onInput={on_input} />
                </p>
                <p><button onClick={submit}>{submit_text}</button></p>
            </Fragment>
        );

        return (
            <Fragment>
                <main>
                    {state.vote.num_players <= 1 && <p>Send this URL to all voters: <code>{window.location.href}</code></p>}
                    {!state.vote.results && ballot_section}
                    {submitted_section}
                    {!state.vote.results && <p><button onClick={tally}>End Voting and Show the Results</button></p>}
                    <p>{state.vote.num_votes}/{state.vote.num_players} voters have submitted ballots.</p>
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
                    <p> Oops! Something went wrong. </p>
                    <p> <a href="javascript:void(0)" onClick={() => location.reload()}>Click here to refresh the page.</a></p>
                    <section>
                        <p> {this.state.error.toString()} </p>
                        <details>
                            <summary> Extra details: </summary>
                            <pre>{this.state.error.stack}</pre>
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
