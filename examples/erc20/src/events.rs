use alloy_sol_types::sol;

sol! {
    #[derive(Debug, PartialEq, Eq)]
    event Transfer(address indexed from, address indexed to, uint256 value);

    interface IERC20Metadata {
        /// Returns the token's display name.
        function name() external view returns (string value);
        /// Returns the token's ticker symbol.
        function symbol() external view returns (string value);
        /// Returns the number of decimal places used for token amounts.
        function decimals() external view returns (uint8 value);
    }
}
