import '@picocss/pico/css/pico.min.css';
import './style.css';
import { Component, createRef } from 'preact';
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
            <main class="container">
                <section>
                    <h2><a href="javascript:void(0)" onclick={rps}>Rock Paper Scissors</a></h2>
                    <h2><a href="javascript:void(0)" onclick={vote}>Condorcet Voting</a></h2>
                </section>
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
            return <footer>Disconnected!</footer>
        }
        let components = [];
        let player_view = state.room_state.player_view;
        let spectator_view = state.room_state.spectator_view;
        const is_player = !!player_view;
        if (is_player) {
            let ws = this.ws;
            const get_onclick = choice => function() {
                ws.send(JSON.stringify({ choice }))
            };
            components.push(
                <div>
                    <a href="javascript:void(0)" role="button" onclick={get_onclick("rock")}>rock</a>
                    {" "}
                    <a href="javascript:void(0)" role="button" onclick={get_onclick("paper")}>paper</a>
                    {" "}
                    <a href="javascript:void(0)" role="button" onclick={get_onclick("scissors")}>scissors</a>
                </div>
            );
            if (player_view.choice) {
                components.push(
                    <div>You have selected: {player_view.choice}.</div>
                );
            }
            components.push(<div>{player_view.opponent_chosen ? "Opponent has selected a choice." : "Waiting for opponent..."}</div>);
        }

        if (is_player && (player_view.wins || player_view.losses || player_view.draws)) {
            components.push(
                <div>Wins: {player_view.wins} Losses: {player_view.losses} Draws: {player_view.draws}</div>
            );
        } else if (!!spectator_view && (spectator_view.player_wins || spectator_view.draws)) {
            components.push(<div> Wins: {spectator_view.player_wins.join(" vs ")} Draws: {spectator_view.draws}</div>)
        }

        components.push(
            <div>There are {state.room_state.num_players} players and {state.room_state.num_spectators} spectators.</div>
        );

        let history = state.room_state.history;
        if (history.length >= 0) {
            let items = [];
            for (let i = 0; i < history.length; i++) {
                let item = `${history[i][0]} vs ${history[i][1]}`;
                if (is_player) {
                    item = `${player_view.outcome_history[i]}: ${item}`;
                }
                items.push(<li>{item}</li>)
            }
            components.push(<ol>{items}</ol>);
        }

        components.push(
            <footer>{is_player ? "You are a player!" : "You are a spectator!"}</footer>
        );
        return components;
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
            const choice_class = this.state.selected == i ? "choice outline contrast" : "choice outline";
            const ondragstart = () => this.onDragStart(i);
            const ondragenter = () => this.onDragEnter(i);
            choice = <a href="javascript:void(0)" role="button" class={choice_class} onclick={choice_onclick} ondragstart={ondragstart} ondragenter={ondragenter}>{choice}</a>;
            choices.push(choice);
            if (i + 1 != props.choices.length) {
                const rank_onclick = () => this.onRankClick(i);
                const symbol = this.state.gt[i] ? "<" : "=";
                let order_elem = <a href="javascript:void(0)" role="button" class="ordering secondary outline" onclick={rank_onclick}>{symbol}</a>;
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
            const symbol = (i == j) ? "-" : (val > totals[j][i]) ? <mark>{val}</mark> : { val };
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
    state = { status: "connecting", voter_name: "???" };
    ws = null;
    room = null;
    choices_component = createRef();

    constructor(props) {
        super();
        this.room = props.room;
    }

    componentDidMount() {
        document.title = "Vote";
        if (this.room) {
            let ws = make_websocket(`/api/vote/${this.room}`);
            ws.onclose = () => this.setState({ status: "disconnected" });
            ws.onmessage = msg => this.setState(JSON.parse(msg.data));
            this.ws = ws;
        }
    }

    render(_props, state) {
        console.log(state);
        if (!this.room) {
            return <form action="/api/start_vote" method="post">
                <label for="choices">Enter the choices up for vote, one per line:</label>
                <textarea name="choices" />
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

        let submitted_section = <div />;
        if (state.vote.your_vote) {
            let description = describe_vote(state.vote.choices, state.vote.your_vote);
            submitted_section = <div>You submitted: {description}</div>;
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

        return (
            <section>
                <p>Edit your ballot by clicking or dragging.</p>
                <Choices ref={this.choices_component} choices={state.vote.choices} />
                <div>
                    <label for="voter_name">Voter name:</label>
                    <input value={state.voter_name} onInput={on_input} />
                </div>
                <button onclick={submit}>Submit Your Vote</button>
                {submitted_section}
                <button onclick={tally}>Tally the Votes</button>
                <div>{state.vote.num_votes}/{state.vote.num_players} voters have submitted ballots.</div>
                {results}
                <a href="/vote">Create a new election.</a>
            </section>
        );
    }
}

export default function App() {
    return (
        <main class="container">
            <Router>
                <Index path="/" />
                <Rps path="/rps/:room?" />
                <Vote path="/vote/:room?" />
            </Router>
        </main>
    );
}
