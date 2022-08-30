import '@picocss/pico/css/pico.min.css';
// import { Component } from 'preact';
import Router from 'preact-router';

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

export default function App() {
    return (
        <Router>
            <Index path="/" />
        </Router>
    );
}
