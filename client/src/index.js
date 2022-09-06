import '@picocss/pico/css/pico.min.css';
import { Component } from 'preact';
import { route, Router } from 'preact-router';

function Index() {
    return (
        <main class="container">
            <section>
                <h2><a href="rps">Rock Paper Scissors</a></h2>
                <h2><a href="vote">Condorcet Voting</a></h2>
            </section>
        </main>
    )
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
        if (!props.room) {
            const room = crypto.randomUUID().substring(0, 5);
            // XXX: preact-router route() doesn't work on initial load.
            // https://github.com/preactjs/preact-router/issues/417
            setTimeout(() => route(`/rps/${room}`));
        }
        this.room = props.room;
    }

    componentDidMount() {
        let ws = make_websocket(`/api/rps/${this.room}`);
        ws.onclose = function() {
            this.setState({ status: "disconnected" });
        }.bind(this);
        ws.onmessage = function(msg) {
            this.setState(JSON.parse(msg.data));
        }.bind(this);
        this.ws = ws;
    }

    render(_props, state) {
        console.log(state);
        if (state.status == "connecting") {
            return <footer id="status">Connecting...</footer>
        } else if (state.status == "disconnected") {
            return <footer id="status">Disconnected!</footer>
        }
        let components = [];
        let player_view = state.room_state.player_view;
        let spectator_view = state.room_state.spectator_view;
        const is_player = !!player_view;
        if (is_player) {
            let ws = this.ws;
            const get_onclick = choice => function() {
                ws.send(JSON.stringify({ "choice": choice }))
            };
            components.push(
                <div>
                    <a href="#" role="button" onclick={get_onclick("rock")}>rock</a>
                    {" "}
                    <a href="#" role="button" onclick={get_onclick("paper")}>paper</a>
                    {" "}
                    <a href="#" role="button" onclick={get_onclick("scissors")}>scissors</a>
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
            for (var i = 0; i < history.length; i++) {
                let item = `${history[i][0]} vs ${history[i][1]}`;
                if (is_player) {
                    item = `${player_view.outcome_history[i]}: ${item}`;
                }
                items.push(<li>{item}</li>)
            }
            components.push(<ol>{items}</ol>);
        }

        components.push(
            <footer id="status">{is_player ? "You are a player!" : "You are a spectator!"}</footer>
        );
        return components;
    }
}

function Vote() {
    return (
        <main class="container" />
    )
}

export default function App() {
    return (
        <main class="container">
            <Router>
                <Index path="/" />
                <Rps path="/rps/:room?" />
                <Vote path="/vote" />
            </Router>
        </main>
    );
}
