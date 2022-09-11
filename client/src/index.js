import './style.css';
import { Component, createRef, Fragment } from 'preact';
import { route, Router } from 'preact-router';

class Index extends Component {
    componentDidMount() {
        document.title = "Decide"
    }

    render() {
        function rps() {
            const room = crypto.randomUUID().substring(0, 5);
            route(`/rps/${room}`);
        }
        function vote() {
            route("/vote/");
        }
        return (
            <main>
                <h2><a href="javascript:void(0)" onclick={rps}>Rock Paper Scissors</a></h2>
                <h2><a href="javascript:void(0)" onclick={vote}>Condorcet Voting</a></h2>
            </main>
        )
    }
}

function make_websocket(path) {
    const ws_protocol = (window.location.protocol == "https:") ? "wss://" : "ws://";
    const uri = ws_protocol + location.host + path;
    return new WebSocket(uri);
}

class Rps extends Component {
    state = { status: "connecting" };
    ws = null;
    room = null;

    constructor(props) {
        super();
        this.room = props.room;
    }

    componentDidMount() {
        document.title = "Rock Paper Scissors"
        let ws = make_websocket(`/api/rps/${this.room}`);
        ws.onclose = () => this.setState({ status: "disconnected" });
        ws.onmessage = msg => this.setState(JSON.parse(msg.data));
        this.ws = ws;
    }

    render(_props, state) {
        console.log(state);
        if (state.status == "connecting") {
            return <footer>Connecting...</footer>
        } else if (state.status == "disconnected") {
            return <footer>Disconnected! Try refreshing.</footer>
        }
        let player_view = state.room_state.player_view;
        let spectator_view = state.room_state.spectator_view;
        const is_player = !!player_view;
        const get_onclick = choice => () => {
            this.ws.send(JSON.stringify({ choice }))
        };

        let history = state.room_state.history;
        let history_component;
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
                {is_player && <div>
                    {(state.room_state.num_players < 2) && <p>Send this URL to your opponent to connect.</p>}
                    <p>
                        <button onclick={get_onclick("rock")}>rock</button>
                        {" "}
                        <button onclick={get_onclick("paper")}>paper</button>
                        {" "}
                        <button onclick={get_onclick("scissors")}>scissors</button>
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
                <footer>
                    <div>There are {state.room_state.num_players} players and {state.room_state.num_spectators} spectators.</div>
                    <div>{is_player ? "You are a player!" : "You are a spectator!"}</div>
                </footer>
            </Fragment>
        );
    }
}

function describe_vote(choices, vote) {
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

class Choices extends Component {
    constructor(props) {
        super();
        this.state = {
            // Mapping of sorted position -> choice index.
            order: Array(props.choices.length).fill().map((_, i) => i),
            // true if sorted choice i is > choice i+1, else false if equal.
            gt: Array(props.choices.length).fill(true),
            selected: null,
        };
    }

    swap(i, j) {
        let order = this.state.order.slice();
        let tmp = order[j];
        order[j] = order[i];
        order[i] = tmp;
        return { order }
    }

    onChoiceClick(i) {
        if (this.state.selected == null) {
            this.setState({ selected: i });
        } else {
            this.setState({ ...this.swap(i, this.state.selected), selected: null });
        }
    }

    onRankClick(i) {
        let gt = this.state.gt.slice();
        gt[i] = !gt[i];
        this.setState({ gt });
    }

    onDragStart(i) {
        this.setState({ selected: i });
    }

    onDragEnter(i) {
        console.assert(this.state.selected != null);
        this.setState({ ...this.swap(i, this.state.selected), selected: i });
    }

    render(props, state) {
        let choices = [];
        for (let i = 0; i < props.choices.length; i++) {
            let choice = props.choices[state.order[i]];
            const choice_onclick = () => this.onChoiceClick(i);
            const choice_class = this.state.selected == i ? "choice chosen" : "choice";
            const ondragstart = () => this.onDragStart(i);
            const ondragenter = () => this.onDragEnter(i);
            choice = <button class={choice_class} draggable="true" onclick={choice_onclick} ondragstart={ondragstart} ondragenter={ondragenter}>{choice}</button>;
            choices.push(choice);
            if (i + 1 != props.choices.length) {
                const rank_onclick = () => this.onRankClick(i);
                const symbol = this.state.gt[i] ? "<" : "=";
                let order_elem = <button class="ordering" onclick={rank_onclick}>{symbol}</button>;
                choices.push(" ");
                choices.push(order_elem);
                choices.push(" ");
            }
        }
        return <div>{choices}</div>;
    }
}

function VoteResults({ choices, results }) {
    const winners = results.tally.winners.map(i => choices[i]).join(" AND ");
    const votes = results.votes.map((v, i) => <li key={i}>{describe_vote(choices, v)}</li>);
    const tchoices = choices.map((c, i) => <th key={i} scope="row">{c}</th>);
    const thead = <thead><tr><th key="head" scope="col" />{tchoices}</tr></thead>;
    const totals = results.tally.totals;
    const trows = totals.map((_, i) => {
        const tds = totals[i].map((val, j) => {
            const symbol = (i == j) ? "-" : (val > totals[j][i]) ? <mark>{val}</mark> : val.toString();
            return <td key={j}>{symbol}</td>
        });
        return <tr key={i}><th key="head" scope="row">{choices[i]}</th>{tds}</tr>
    });
    return <article>
        <header>The results are in! The winner is: <strong>{winners}</strong></header>
        <p> The votes are: </p>
        <ul>
            {votes}
        </ul>
        <table role="grid">
            {thead}
            {trows}
        </table>
    </article>

}

class Vote extends Component {
    state = { room: null, status: "connecting", voter_name: "???" };
    ws = null;
    choices_component = createRef();

    componentDidMount() {
        document.title = "Vote";
    }

    render(props, state) {
        if (state.room != props.room) {
            state.room = props.room;
            this.ws = make_websocket(`/api/vote/${state.room}`);
            this.ws.onclose = () => this.setState({ status: "disconnected" });
            this.ws.onmessage = msg => this.setState(JSON.parse(msg.data));
        }
        console.log(state);
        if (!state.room) {
            return <form action="/api/start_vote" method="post">
                <p><label for="choices">Enter the choices up for vote, one per line:</label></p>
                <p><textarea name="choices" /></p>
                <input type="submit" value="Start Vote" />
            </form>
        } else if (state.status == "connecting") {
            return <footer>Connecting...</footer>
        } else if (state.status == "disconnected") {
            return <footer>Disconnected! Try refreshing.</footer>
        } else if (state.status == "invalid_room") {
            route("/vote");
            return <footer>Invalid room!</footer>
        }

        const on_input = event => this.setState({ voter_name: event.target.value });

        let submitted_section = null;
        if (state.vote.your_vote) {
            let description = describe_vote(state.vote.choices, state.vote.your_vote);
            submitted_section = <p>You submitted: {description}</p>;
        }

        const submit = () => {
            const choices_component = this.choices_component.current;
            let items = [];
            let rank = 0;
            for (let i = 0; i < state.vote.choices.length; i++) {
                let item = choices_component.state.order[i];
                items.push({ candidate: item, rank });
                if (choices_component.state.gt[i]) {
                    rank++;
                }
            }
            this.ws.send(JSON.stringify({ vote: { name: this.state.voter_name, selections: items } }))
        };

        const tally = () => this.ws.send(JSON.stringify({ tally: null }));

        let results = null;
        if (state.vote.results) {
            results = <VoteResults choices={state.vote.choices} results={state.vote.results} />
        }

        const ballot_section = (
            <Fragment>
                <p>Click or drag to edit your ballot.</p>
                <Choices ref={this.choices_component} choices={state.vote.choices} />
                <p>
                    <label for="voter_name">Voter name:</label>
                    <input value={state.voter_name} onInput={on_input} />
                </p>
                <p><button onclick={submit}>Submit Your Vote</button></p>
            </Fragment>
        );

        return (
            <Fragment>
                {!state.vote.results && ballot_section}
                {submitted_section}
                {!state.vote.results && <p><button onclick={tally}>Tally the Votes</button></p>}
                <p>{state.vote.num_votes}/{state.vote.num_players} voters have submitted ballots.</p>
                {results}
                <a href="/vote">Create a new election.</a>
            </Fragment>
        );
    }
}

export default function App() {
    return (
        <main>
            <Router>
                <Index path="/" />
                <Rps path="/rps/:room?" />
                <Vote path="/vote/:room?" />
            </Router>
        </main>
    );
}
