module.exports = function(config) {
    if (process.env.NODE_ENV === 'development' && config.devServer) {
        config.devServer.proxy = [
            {
                context: ['/api/'],
                target: 'ws://localhost:8000',
                ws: true

            }
        ];
    }
};
