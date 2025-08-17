import { Component, Fragment, VNode } from 'preact';
import { make_websocket } from './websocket';
import { CopyLink } from './copylink';

type RpsProps = {
    room: string
}

type RpsState = {
    room: string,
    status: string,
    room_state: RoomView | null
}

type RoomView = {
    num_players: number
    num_spectators: number
    history: string[][]
    player_view: PlayerView | null
    spectator_view: SpectatorView | null
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

export class Rps extends Component<RpsProps, RpsState> {
    state = { room: "", status: "connecting", room_state: null };
    ws: WebSocket | null = null;

    componentDidMount() {
        document.title = "Rock Paper Scissors"
    }

    componentWillUnmount() {
        try {
            if (this.ws != null) {
                this.ws.close();
            }
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

