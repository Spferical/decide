import '@picocss/pico/css/pico.min.css';
import { render, Component } from 'preact';

export default class App extends Component {
    render() {
        return (
            <main class="container">
                <section>
                    <h2><a href="rps">Rock Paper Scissors</a></h2>
                    <h2><a href="vote">Condorcet Voting</a></h2>
                </section>
            </main>
        );
    }
}
