use alloy_sol_types::sol;
sol! {
    #[derive(Debug, PartialEq, Eq)]
    event PoolCreated(address indexed token0, address indexed token1, uint24 indexed fee, int24 tickSpacing, address pool);
    #[derive(Debug, PartialEq, Eq)]
    event Initialize(uint160 sqrtPriceX96, int24 tick);
    #[derive(Debug, PartialEq, Eq)]
    event Swap(address indexed sender, address indexed recipient, int256 amount0, int256 amount1, uint160 sqrtPriceX96, uint128 liquidity, int24 tick);
    #[derive(Debug, PartialEq, Eq)]
    event Mint(address sender, address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1);
    #[derive(Debug, PartialEq, Eq)]
    event Burn(address indexed owner, int24 indexed tickLower, int24 indexed tickUpper, uint128 amount, uint256 amount0, uint256 amount1);
    #[derive(Debug, PartialEq, Eq)]
    event Flash(address indexed sender, address indexed recipient, uint256 amount0, uint256 amount1, uint256 paid0, uint256 paid1);
    interface PositionManager {
        #[derive(Debug, PartialEq, Eq)]
        event IncreaseLiquidity(uint256 indexed tokenId, uint128 liquidity, uint256 amount0, uint256 amount1);
        #[derive(Debug, PartialEq, Eq)]
        event DecreaseLiquidity(uint256 indexed tokenId, uint128 liquidity, uint256 amount0, uint256 amount1);
        #[derive(Debug, PartialEq, Eq)]
        event Collect(uint256 indexed tokenId, address recipient, uint256 amount0, uint256 amount1);
        #[derive(Debug, PartialEq, Eq)]
        event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
        /// Returns the position-manager state for an NFT token ID.
        function positions(uint256 tokenId) external view returns (
            uint96 nonce, address operator, address token0, address token1, uint24 fee,
            int24 tickLower, int24 tickUpper, uint128 liquidity,
            uint256 feeGrowthInside0LastX128, uint256 feeGrowthInside1LastX128,
            uint128 tokensOwed0, uint128 tokensOwed1);
    }
    interface FactoryMetadata {
        /// Returns the pool address for a token pair and fee tier.
        function getPool(address tokenA, address tokenB, uint24 fee) external view returns (address pool);
    }
    interface TokenMetadata {
        /// Returns the token name.
        function name() external view returns (string value);
        /// Returns the token symbol.
        function symbol() external view returns (string value);
        /// Returns the token decimal scale.
        function decimals() external view returns (uint32 value);
        /// Returns the token total supply.
        function totalSupply() external view returns (uint256 value);
    }
    interface BytesMetadata {
        /// Returns the bytes32 token name fallback.
        function name() external view returns (bytes32 value);
        /// Returns the bytes32 token symbol fallback.
        function symbol() external view returns (bytes32 value);
    }
    interface PoolMetadata {
        /// Returns global fee growth for token zero.
        function feeGrowthGlobal0X128() external view returns (uint256 value);
        /// Returns global fee growth for token one.
        function feeGrowthGlobal1X128() external view returns (uint256 value);
        /// Returns the state for a pool tick index.
        function ticks(int24 tick) external view returns (
            uint128 liquidityGross, int128 liquidityNet, uint256 feeGrowthOutside0X128,
            uint256 feeGrowthOutside1X128, int56 tickCumulativeOutside,
            uint160 secondsPerLiquidityOutsideX128, uint32 secondsOutside, bool initialized);
    }
}
