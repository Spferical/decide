import { Component, Fragment, render } from 'preact';
import { route, Router } from 'preact-router';
import { v4 as uuidv4 } from 'uuid';

import { Rps } from './rps';
import { Vote } from './vote';

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
